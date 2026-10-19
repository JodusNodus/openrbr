//! Parses the call trace named by `RBR_TRACE`. Skipped when the variable is unset,
//! so `cargo test` works without a trace.

use rbr_recording::trace::Calls;

#[test]
fn trace_parses_call_by_call() {
    let Ok(path) = std::env::var("RBR_TRACE") else {
        eprintln!("skipping: RBR_TRACE not set");
        return;
    };
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let calls: Vec<_> = Calls::new(&bytes)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .collect::<Result<_, _>>()
        .unwrap_or_else(|e| panic!("{path}: {e}"));
    assert!(!calls.is_empty(), "{path}: no calls");
    for call in &calls {
        for region in &call.regions {
            assert_eq!(region.pre.len(), region.post.len(), "call at offset {}", call.offset);
        }
    }
}
