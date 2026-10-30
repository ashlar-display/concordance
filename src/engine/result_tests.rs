use super::*;

#[test]
fn warnings_keep_order_and_count() {
    let mut w = Warnings::default();
    assert!(w.is_empty());
    w.push(1);
    w.push(2);
    assert_eq!(w.iter().copied().collect::<heapless_vec::V>(), [1, 2]);
    assert_eq!((w.len(), w.dropped()), (2, 0));
}

#[cfg(not(any(feature = "alloc", feature = "std")))]
#[test]
fn warnings_beyond_capacity_are_counted() {
    let mut w = Warnings::new();
    for i in 0..MAX_WARNINGS + 3 {
        w.push(i);
    }
    assert_eq!((w.len(), w.dropped()), (MAX_WARNINGS, 3));
}

#[test]
fn violations_first_and_completeness() {
    let mut v = Violations::new('a');
    v.push('b');
    assert_eq!(*v.first(), 'a');
    assert!(!v.is_empty());
    assert!(v.is_complete());
    #[cfg(any(feature = "alloc", feature = "std"))]
    assert_eq!((v.len(), v.dropped()), (2, 0));
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    assert_eq!((v.len(), v.dropped()), (1, 1));
    assert!(!Violations::new('a').stopped_early().is_complete());
}

/// A fixed-capacity collector, so the order test runs in every build.
mod heapless_vec {
    #[derive(Debug, PartialEq)]
    pub struct V([i32; 2], usize);
    impl FromIterator<i32> for V {
        fn from_iter<I: IntoIterator<Item = i32>>(iter: I) -> Self {
            let mut v = V([0; 2], 0);
            for x in iter {
                v.0[v.1] = x;
                v.1 += 1;
            }
            v
        }
    }
    impl PartialEq<[i32; 2]> for V {
        fn eq(&self, other: &[i32; 2]) -> bool {
            self.1 == 2 && &self.0 == other
        }
    }
}
