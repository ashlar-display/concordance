//! The result of a constraint check: [`Warnings`] for an accepted candidate, [`Violations`]
//! for a rejected one.
//!
//! Both keep their storage private and offer the same API in every build: with `alloc`
//! they hold everything in a `Vec`; without it, [`Warnings`] holds up to
//! [`MAX_WARNINGS`][super::MAX_WARNINGS] and counts the rest, and [`Violations`] holds the first violation and
//! counts any later one. `alloc` only adds methods (`into_vec`).

#[cfg(any(feature = "alloc", feature = "std"))]
use alloc::vec::Vec;

#[cfg(not(any(feature = "alloc", feature = "std")))]
use super::MAX_WARNINGS;

/// Warnings for a candidate the engine accepted.
#[derive(Debug, Clone, PartialEq)]
pub struct Warnings<W> {
    #[cfg(any(feature = "alloc", feature = "std"))]
    items: Vec<W>,
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    items: [Option<W>; MAX_WARNINGS],
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    len: usize,
    dropped: u16,
}

impl<W> Default for Warnings<W> {
    fn default() -> Self {
        Self::new()
    }
}

impl<W> Warnings<W> {
    /// No warnings.
    pub fn new() -> Self {
        Self {
            #[cfg(any(feature = "alloc", feature = "std"))]
            items: Vec::new(),
            #[cfg(not(any(feature = "alloc", feature = "std")))]
            items: core::array::from_fn(|_| None),
            #[cfg(not(any(feature = "alloc", feature = "std")))]
            len: 0,
            dropped: 0,
        }
    }

    /// Adds a warning. Without alloc, a warning beyond [`MAX_WARNINGS`][super::MAX_WARNINGS] is not stored but
    /// counted in [`dropped`][Self::dropped].
    pub fn push(&mut self, w: W) {
        #[cfg(any(feature = "alloc", feature = "std"))]
        self.items.push(w);
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        if self.len < MAX_WARNINGS {
            self.items[self.len] = Some(w);
            self.len += 1;
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    /// The stored warnings, in the order they were added.
    pub fn iter(&self) -> impl Iterator<Item = &W> {
        #[cfg(any(feature = "alloc", feature = "std"))]
        return self.items.iter();
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        self.items[..self.len].iter().flatten()
    }

    /// The number of stored warnings.
    pub fn len(&self) -> usize {
        #[cfg(any(feature = "alloc", feature = "std"))]
        return self.items.len();
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        self.len
    }

    /// Whether no warning is stored (and none was dropped).
    pub fn is_empty(&self) -> bool {
        self.len() == 0 && self.dropped == 0
    }

    /// The number of warnings that did not fit (saturating at `u16::MAX`). Always 0 with
    /// alloc.
    pub fn dropped(&self) -> u16 {
        self.dropped
    }

    /// The warnings as a `Vec`.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub fn into_vec(self) -> Vec<W> {
        self.items
    }
}

/// Violations for a candidate the engine rejected; never empty.
#[derive(Debug, Clone, PartialEq)]
pub struct Violations<V> {
    #[cfg(any(feature = "alloc", feature = "std"))]
    items: Vec<V>,
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    first: V,
    dropped: u16,
    complete: bool,
}

impl<V> Violations<V> {
    /// A rejection with its first violation.
    pub fn new(first: V) -> Self {
        Self {
            #[cfg(any(feature = "alloc", feature = "std"))]
            items: alloc::vec![first],
            #[cfg(not(any(feature = "alloc", feature = "std")))]
            first,
            dropped: 0,
            complete: true,
        }
    }

    /// Adds a further violation. Without alloc only the first is stored; later ones are
    /// counted in [`dropped`][Self::dropped].
    pub fn push(&mut self, v: V) {
        #[cfg(any(feature = "alloc", feature = "std"))]
        self.items.push(v);
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        {
            let _ = v;
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    /// Marks that the engine stopped at this violation without running the remaining
    /// rules, so other violations may exist ([`is_complete`][Self::is_complete] is then
    /// `false`).
    pub fn stopped_early(mut self) -> Self {
        self.complete = false;
        self
    }

    /// The first violation.
    pub fn first(&self) -> &V {
        #[cfg(any(feature = "alloc", feature = "std"))]
        return &self.items[0];
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        &self.first
    }

    /// The stored violations, in the order they were found.
    pub fn iter(&self) -> impl Iterator<Item = &V> {
        #[cfg(any(feature = "alloc", feature = "std"))]
        return self.items.iter();
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        core::iter::once(&self.first)
    }

    /// The number of stored violations (at least 1).
    pub fn len(&self) -> usize {
        #[cfg(any(feature = "alloc", feature = "std"))]
        return self.items.len();
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        1
    }

    /// Always `false`: a rejection has at least one violation.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// The number of violations found but not stored (saturating). Always 0 with alloc.
    pub fn dropped(&self) -> u16 {
        self.dropped
    }

    /// Whether every rule ran, so the stored and dropped violations are all there are.
    /// `false` when the engine stopped at the first violation (the default engine without
    /// alloc does).
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// The violations as a `Vec`.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub fn into_vec(self) -> Vec<V> {
        self.items
    }
}

#[cfg(test)]
mod tests {
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
}
