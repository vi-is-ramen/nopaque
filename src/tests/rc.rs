use crate::rc;

struct DropMe {
    _dummy: u8,
}

#[test]
fn rc_creation_and_clone() {
    let r = <rc!(&DropMe)>::new(DropMe { _dummy: 10 });
    let r2 = r.clone();
    assert_eq!(r._dummy, 10);
    assert_eq!(r2._dummy, 10);
    unsafe {
        assert_eq!(r.rc_load(), 2);
        assert_eq!(r2.rc_load(), 2);
    }
}
