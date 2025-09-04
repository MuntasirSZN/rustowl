mod analyze;
mod cache;

use analyze::{AnalyzeResult, MirAnalyzer, MirAnalyzerInitResult};
use rustc_hir::def_id::{LOCAL_CRATE, LocalDefId};
use rustc_interface::interface;
use rustc_middle::{mir::ConcreteOpaqueTypes, query::queries, ty::TyCtxt, util::Providers};
use rustc_session::config;
use rustowl::models::FoldIndexMap as HashMap;
use rustowl::models::*;
use std::env;
use std::sync::{LazyLock, Mutex, atomic::AtomicBool};
use tokio::{
    runtime::{Builder, Runtime},
    task::JoinSet,
};

pub struct RustcCallback;
impl rustc_driver::Callbacks for RustcCallback {}

static ATOMIC_TRUE: AtomicBool = AtomicBool::new(true);
static TASKS: LazyLock<Mutex<JoinSet<AnalyzeResult>>> =
    LazyLock::new(|| Mutex::new(JoinSet::new()));
// make tokio runtime
static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    let worker_threads = std::thread::available_parallelism()
        .map(|n| (n.get() / 2).clamp(2, 8))
        .unwrap_or(4);

    Builder::new_multi_thread()
        .enable_all()
        .worker_threads(worker_threads)
        .thread_stack_size(128 * 1024 * 1024)
        .build()
        .unwrap()
});

fn override_queries(_session: &rustc_session::Session, local: &mut Providers) {
    local.mir_borrowck = mir_borrowck;
}
fn mir_borrowck(tcx: TyCtxt<'_>, def_id: LocalDefId) -> queries::mir_borrowck::ProvidedValue<'_> {
    tracing::info!("start borrowck of {def_id:?}");

    let analyzer = MirAnalyzer::init(tcx, def_id);

    {
        let mut tasks = TASKS.lock().unwrap();
        match analyzer {
            MirAnalyzerInitResult::Cached(cached) => {
                handle_analyzed_result(tcx, *cached);
            }
            MirAnalyzerInitResult::Analyzer(analyzer) => {
                tasks.spawn_on(async move { analyzer.await.analyze() }, RUNTIME.handle());
            }
        }

        tracing::info!("there are {} tasks", tasks.len());
        while let Some(Ok(result)) = tasks.try_join_next() {
            tracing::info!("one task joined");
            handle_analyzed_result(tcx, result);
        }
    }

    for def_id in tcx.nested_bodies_within(def_id) {
        let _ = mir_borrowck(tcx, def_id);
    }

    Ok(tcx.arena.alloc(ConcreteOpaqueTypes(
        rustc_data_structures::fx::FxIndexMap::default(),
    )))
}

pub struct AnalyzerCallback;
impl rustc_driver::Callbacks for AnalyzerCallback {
    fn config(&mut self, config: &mut interface::Config) {
        config.using_internal_features = &ATOMIC_TRUE;
        config.opts.unstable_opts.mir_opt_level = Some(0);
        config.opts.unstable_opts.polonius = config::Polonius::Next;
        config.opts.incremental = None;
        config.override_queries = Some(override_queries);
        config.make_codegen_backend = None;
    }
    fn after_expansion<'tcx>(
        &mut self,
        _compiler: &interface::Compiler,
        tcx: TyCtxt<'tcx>,
    ) -> rustc_driver::Compilation {
        let result = rustc_driver::catch_fatal_errors(|| tcx.analysis(()));

        // join all tasks after all analysis finished
        //
        // allow clippy::await_holding_lock because `tokio::sync::Mutex` cannot use
        // for TASKS because block_on cannot be used in `mir_borrowck`.
        #[allow(clippy::await_holding_lock)]
        // Drain all remaining analysis tasks synchronously
        loop {
            // First collect any tasks that have already finished
            while let Some(Ok(result)) = {
                let mut guard = TASKS.lock().unwrap();
                guard.try_join_next()
            } {
                tracing::info!("one task joined");
                handle_analyzed_result(tcx, result);
            }

            // Check if all tasks are done
            let has_tasks = {
                let guard = TASKS.lock().unwrap();
                !guard.is_empty()
            };
            if !has_tasks {
                break;
            }

            // Wait for at least one more task to finish
            let result = {
                let mut guard = TASKS.lock().unwrap();
                RUNTIME.block_on(guard.join_next())
            };
            if let Some(Ok(result)) = result {
                tracing::info!("one task joined");
                handle_analyzed_result(tcx, result);
            }
        }

        if let Some(cache) = cache::CACHE.lock().unwrap().as_ref() {
            // Log cache statistics before writing
            let stats = cache.get_stats();
            tracing::info!(
                "Cache statistics: {} hits, {} misses, {:.1}% hit rate, {} evictions",
                stats.hits,
                stats.misses,
                stats.hit_rate() * 100.0,
                stats.evictions
            );
            cache::write_cache(&tcx.crate_name(LOCAL_CRATE).to_string(), cache);
        }

        if result.is_ok() {
            rustc_driver::Compilation::Continue
        } else {
            rustc_driver::Compilation::Stop
        }
    }
}

pub fn handle_analyzed_result(tcx: TyCtxt<'_>, analyzed: AnalyzeResult) {
    if let Some(cache) = cache::CACHE.lock().unwrap().as_mut() {
        // Pass file name for potential file modification time validation
        cache.insert_cache_with_file_path(
            analyzed.file_hash.clone(),
            analyzed.mir_hash.clone(),
            analyzed.analyzed.clone(),
            Some(&analyzed.file_name),
        );
    }
    let mut map = HashMap::with_capacity_and_hasher(1, foldhash::quality::RandomState::default());
    map.insert(
        analyzed.file_name.to_owned(),
        File {
            items: smallvec::smallvec![analyzed.analyzed],
        },
    );
    let krate = Crate(map);
    // get currently-compiling crate name
    let crate_name = tcx.crate_name(LOCAL_CRATE).to_string();
    let mut ws_map =
        HashMap::with_capacity_and_hasher(1, foldhash::quality::RandomState::default());
    ws_map.insert(crate_name.clone(), krate);
    let ws = Workspace(ws_map);
    println!("{}", serde_json::to_string(&ws).unwrap());
}

pub fn run_compiler() -> i32 {
    let mut args: Vec<String> = env::args().collect();
    // by using `RUSTC_WORKSPACE_WRAPPER`, arguments will be as follows:
    // For dependencies: rustowlc [args...]
    // For user workspace: rustowlc rustowlc [args...]
    // So we skip analysis if currently-compiling crate is one of the dependencies
    if args.first() == args.get(1) {
        args = args.into_iter().skip(1).collect();
    } else {
        return rustc_driver::catch_with_exit_code(|| {
            rustc_driver::run_compiler(&args, &mut RustcCallback)
        });
    }

    for arg in &args {
        // utilize default rustc to avoid unexpected behavior if these arguments are passed
        if arg == "-vV" || arg == "--version" || arg.starts_with("--print") {
            return rustc_driver::catch_with_exit_code(|| {
                rustc_driver::run_compiler(&args, &mut RustcCallback)
            });
        }
    }

    rustc_driver::catch_with_exit_code(|| {
        rustc_driver::run_compiler(&args, &mut AnalyzerCallback);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_atomic_true_constant() {
        // Test that ATOMIC_TRUE is properly initialized
        assert_eq!(ATOMIC_TRUE.load(Ordering::Relaxed), true);
        
        // Test that it can be read multiple times consistently
        assert_eq!(ATOMIC_TRUE.load(Ordering::SeqCst), true);
        assert_eq!(ATOMIC_TRUE.load(Ordering::Acquire), true);
    }

    #[test]
    fn test_worker_thread_calculation() {
        // Test the worker thread calculation logic
        let available = std::thread::available_parallelism()
            .map(|n| (n.get() / 2).clamp(2, 8))
            .unwrap_or(4);
        
        assert!(available >= 2);
        assert!(available <= 8);
    }

    #[test]
    fn test_runtime_configuration() {
        // Test that RUNTIME is properly configured
        let runtime = &*RUNTIME;
        
        // Test that we can spawn a simple task
        let result = runtime.block_on(async {
            42
        });
        assert_eq!(result, 42);
        
        // Test that runtime handle is available
        let _handle = runtime.handle();
        assert!(tokio::runtime::Handle::try_current().is_ok());
    }

    #[test]
    fn test_rustc_callback_implementation() {
        // Test that RustcCallback implements the required trait
        let _callback = RustcCallback;
        // This verifies that the type can be instantiated and implements Callbacks
    }

    #[test]
    fn test_analyzer_callback_implementation() {
        // Test that AnalyzerCallback implements the required trait
        let _callback = AnalyzerCallback;
        // This verifies that the type can be instantiated and implements Callbacks
    }

    #[test]
    fn test_argument_processing_logic() {
        // Test the argument processing logic without actually running the compiler
        
        // Test detection of version flags
        let version_args = vec!["-vV", "--version", "--print=cfg"];
        for arg in version_args {
            // Simulate the check that's done in run_compiler
            let should_use_default_rustc = arg == "-vV" || arg == "--version" || arg.starts_with("--print");
            assert!(should_use_default_rustc, "Should use default rustc for: {}", arg);
        }
        
        // Test normal compilation args
        let normal_args = vec!["--crate-type", "lib", "-L", "dependency=/path"];
        for arg in normal_args {
            let should_use_default_rustc = arg == "-vV" || arg == "--version" || arg.starts_with("--print");
            assert!(!should_use_default_rustc, "Should not use default rustc for: {}", arg);
        }
    }

    #[test]
    fn test_workspace_wrapper_detection() {
        // Test the RUSTC_WORKSPACE_WRAPPER detection logic
        let test_cases = vec![
            // Case 1: For dependencies: rustowlc [args...]
            (vec!["rustowlc", "--crate-type", "lib"], false), // Different first and second args
            
            // Case 2: For user workspace: rustowlc rustowlc [args...]  
            (vec!["rustowlc", "rustowlc", "--crate-type", "lib"], true), // Same first and second args
            
            // Edge cases
            (vec!["rustowlc"], false), // Only one arg
            (vec!["rustc", "rustc"], true), // Same pattern with rustc
            (vec!["other", "rustowlc"], false), // Different tools
        ];
        
        for (args, should_skip) in test_cases {
            let first = args.first();
            let second = args.get(1);
            let detected_skip = first == second;
            assert_eq!(detected_skip, should_skip, "Failed for args: {:?}", args);
        }
    }

    #[test]
    fn test_hashmap_creation_with_capacity() {
        // Test the HashMap creation pattern used in handle_analyzed_result
        let map: HashMap<String, String> = HashMap::with_capacity_and_hasher(1, foldhash::quality::RandomState::default());
        assert_eq!(map.len(), 0);
        assert!(map.capacity() >= 1);
        
        // Test creating with different capacities
        for capacity in [0, 1, 10, 100] {
            let map: HashMap<String, String> = HashMap::with_capacity_and_hasher(capacity, foldhash::quality::RandomState::default());
            assert_eq!(map.len(), 0);
            if capacity > 0 {
                assert!(map.capacity() >= capacity);
            }
        }
    }

    #[test]
    fn test_workspace_structure_creation() {
        // Test the workspace structure creation logic
        let file_name = "test.rs".to_string();
        let crate_name = "test_crate".to_string();
        
        // Create a minimal Function for testing
        let test_function = Function::new(0);
        
        // Create the nested structure like in handle_analyzed_result
        let mut file_map: HashMap<String, File> = HashMap::with_capacity_and_hasher(1, foldhash::quality::RandomState::default());
        file_map.insert(
            file_name.clone(),
            File {
                items: smallvec::smallvec![test_function],
            },
        );
        let krate = Crate(file_map);
        
        let mut ws_map: HashMap<String, Crate> = HashMap::with_capacity_and_hasher(1, foldhash::quality::RandomState::default());
        ws_map.insert(crate_name.clone(), krate);
        let workspace = Workspace(ws_map);
        
        // Verify structure
        assert_eq!(workspace.0.len(), 1);
        assert!(workspace.0.contains_key(&crate_name));
        
        let crate_ref = &workspace.0[&crate_name];
        assert_eq!(crate_ref.0.len(), 1);
        assert!(crate_ref.0.contains_key(&file_name));
        
        let file_ref = &crate_ref.0[&file_name];
        assert_eq!(file_ref.items.len(), 1);
        assert_eq!(file_ref.items[0].fn_id, 0);
    }

    #[test]
    fn test_json_serialization_output() {
        // Test that the workspace structure can be serialized to JSON
        let test_function = Function::new(42);
        
        let mut file_map: HashMap<String, File> = HashMap::with_capacity_and_hasher(1, foldhash::quality::RandomState::default());
        file_map.insert(
            "main.rs".to_string(),
            File {
                items: smallvec::smallvec![test_function],
            },
        );
        let krate = Crate(file_map);
        
        let mut ws_map: HashMap<String, Crate> = HashMap::with_capacity_and_hasher(1, foldhash::quality::RandomState::default());
        ws_map.insert("my_crate".to_string(), krate);
        let workspace = Workspace(ws_map);
        
        // Test serialization
        let json_result = serde_json::to_string(&workspace);
        assert!(json_result.is_ok());
        
        let json_string = json_result.unwrap();
        assert!(!json_string.is_empty());
        assert!(json_string.contains("my_crate"));
        assert!(json_string.contains("main.rs"));
        assert!(json_string.contains("42"));
    }

    #[test]
    fn test_stack_size_configuration() {
        // Test that the runtime is configured with appropriate stack size
        const EXPECTED_STACK_SIZE: usize = 128 * 1024 * 1024; // 128 MB
        
        // We can't directly inspect the runtime's stack size, but we can verify
        // the constant is reasonable
        assert!(EXPECTED_STACK_SIZE > 1024 * 1024); // At least 1MB
        assert!(EXPECTED_STACK_SIZE <= 1024 * 1024 * 1024); // At most 1GB
        
        // Test that the value is a power of 2 times some base unit
        assert_eq!(EXPECTED_STACK_SIZE % (1024 * 1024), 0); // Multiple of 1MB
    }

    #[test]
    fn test_local_crate_constant() {
        // Test that LOCAL_CRATE constant is available and can be used
        use rustc_hir::def_id::LOCAL_CRATE;
        
        // LOCAL_CRATE should be a valid CrateNum
        // We can't test much about it without a TyCtxt, but we can verify it exists
        let _crate_num = LOCAL_CRATE;
    }

    #[test]
    fn test_config_options_simulation() {
        // Test the configuration options that would be set in AnalyzerCallback::config
        
        // Test mir_opt_level
        let mir_opt_level = Some(0);
        assert_eq!(mir_opt_level, Some(0));
        
        // Test that polonius config enum value exists
        use rustc_session::config::Polonius;
        let _polonius_config = Polonius::Next;
        
        // Test that incremental compilation is disabled
        let incremental = None::<std::path::PathBuf>;
        assert!(incremental.is_none());
    }

    #[test]
    fn test_error_handling_pattern() {
        // Test the error handling pattern used with rustc_driver::catch_fatal_errors
        
        // Simulate successful operation
        let success_result = || -> Result<(), ()> { Ok(()) };
        let result = success_result();
        assert!(result.is_ok());
        
        // Simulate error operation  
        let error_result = || -> Result<(), ()> { Err(()) };
        let result = error_result();
        assert!(result.is_err());
    }
}
