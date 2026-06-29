use crate::{rc_drop, ExplicitDrop};
use std::sync::atomic::{AtomicUsize, Ordering};

static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

struct ExplicitDropMe {
    _dummy: u8,
}
impl ExplicitDrop for ExplicitDropMe {
    fn drop(&mut self) {
        DROP_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn rc_drop_new_calls_explicit_drop() {
    DROP_COUNT.store(0, Ordering::Relaxed);
    let r = <rc_drop!(&ExplicitDropMe)>::new(ExplicitDropMe { _dummy: 0 });
    drop(r);
    assert_eq!(DROP_COUNT.load(Ordering::Relaxed), 1);
}

#[test]
fn rc_drop_clone_and_drop() {
    DROP_COUNT.store(0, Ordering::Relaxed);
    let r = <rc_drop!(&ExplicitDropMe)>::new(ExplicitDropMe { _dummy: 1 });
    let r2 = r.clone();
    unsafe {
        assert_eq!(r.rc_load(), 2);
        assert_eq!(r2.rc_load(), 2);
    }
    drop(r);
    unsafe {
        assert_eq!(r2.rc_load(), 1);
    }
    drop(r2);
    assert_eq!(DROP_COUNT.load(Ordering::Relaxed), 1);
}
