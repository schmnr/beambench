use glib::variant::ToVariant;

fn main() {
    // This must also pass in release mode: the original immutable C output
    // pointer is optimized to null, causing a crash in these safe Rust calls.
    let expected = ["first", "middle", "UTF-8: café", "last"];
    let value = expected.to_variant();
    assert_eq!(
        value.array_iter_str().unwrap().collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        value.array_iter_str().unwrap().rev().collect::<Vec<_>>(),
        expected.into_iter().rev().collect::<Vec<_>>()
    );
    let mut mixed = value.array_iter_str().unwrap();
    assert_eq!(mixed.next(), Some(expected[0]));
    assert_eq!(mixed.next_back(), Some(expected[3]));
    assert_eq!(mixed.next(), Some(expected[1]));
    assert_eq!(mixed.next_back(), Some(expected[2]));
    assert_eq!(mixed.next(), None);
    assert_eq!(value.array_iter_str().unwrap().nth(2), Some(expected[2]));
    assert_eq!(
        value.array_iter_str().unwrap().nth_back(2),
        Some(expected[1])
    );
    assert_eq!(value.array_iter_str().unwrap().last(), Some(expected[3]));
    assert_eq!(value.array_iter_str().unwrap().count(), expected.len());
    println!("variant_str_iter=ok");
}
