use crate::boxed;

struct DropMe {
    _dummy: u8,
}

#[test]
fn box_creation_and_deref() {
    let b = <boxed!(&DropMe)>::new(DropMe { _dummy: 42 });
    assert_eq!(b._dummy, 42);
}
