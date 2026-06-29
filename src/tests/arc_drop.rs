use crate::{arc_drop, ExplicitDrop};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

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
fn arc_drop_send_sync() {
    let a = <arc_drop!(&ExplicitDropMe)>::new(ExplicitDropMe { _dummy: 42 });
    let handle = thread::spawn(move || {
        assert_eq!(a._dummy, 42);
    });
    handle.join().unwrap();
}

#[test]
fn arc_drop_clone_and_count() {
    let a = <arc_drop!(&ExplicitDropMe)>::new(ExplicitDropMe { _dummy: 0 });
    let a2 = a.clone();
    unsafe {
        assert_eq!(a.rc_load(), 2);
        assert_eq!(a2.rc_load(), 2);
    }
    drop(a);
    unsafe {
        assert_eq!(a2.rc_load(), 1);
    }
    drop(a2);
    assert_eq!(DROP_COUNT.load(Ordering::Relaxed), 1);
}
