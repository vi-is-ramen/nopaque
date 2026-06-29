use crate::{boxed_drop, ExplicitDrop};
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
fn box_drop_new_calls_explicit_drop() {
    let b = <boxed_drop!(&ExplicitDropMe)>::new(ExplicitDropMe { _dummy: 0 });
    drop(b);
    assert_eq!(DROP_COUNT.load(Ordering::Relaxed), 1);
}

#[test]
fn box_drop_custom_drop_function() {
    static CUSTOM_DROP: AtomicUsize = AtomicUsize::new(0);
    fn my_drop(_ptr: &mut ()) {
        CUSTOM_DROP.fetch_add(1, Ordering::Relaxed);
    }
    let b = <boxed_drop!(&ExplicitDropMe)>::new_with_drop(ExplicitDropMe { _dummy: 0 }, my_drop);
    drop(b);
    assert_eq!(CUSTOM_DROP.load(Ordering::Relaxed), 1);
}
