use crate::arc;
use std::thread;

struct DropMe {
    _dummy: u8,
}

#[test]
fn arc_creation_and_clone() {
    let a = <arc!(&DropMe)>::new(DropMe { _dummy: 0 });
    let a2 = a.clone();
    unsafe {
        assert_eq!(a.rc_load(), 2);
        assert_eq!(a2.rc_load(), 2);
    }
}

#[test]
fn arc_send_sync() {
    let a = <arc!(&DropMe)>::new(DropMe { _dummy: 42 });
    let handle = thread::spawn(move || {
        assert_eq!(a._dummy, 42);
    });
    handle.join().unwrap();
}
