use super::Array;
use std::{alloc::Allocator, mem::ManuallyDrop};

pub struct IntoIter<T, A: Allocator> {
    pub(super) storage: ManuallyDrop<Array<T, A>>,
    pub(super) start: usize,
}

impl<'s, T, A: Allocator> IntoIterator for &'s Array<T, A> {
    type IntoIter = core::slice::Iter<'s, T>;
    type Item = &'s T;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'s, T, A: Allocator> IntoIterator for &'s mut Array<T, A> {
    type IntoIter = core::slice::IterMut<'s, T>;
    type Item = &'s mut T;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T, A: Allocator> IntoIterator for Array<T, A> {
    type IntoIter = IntoIter<T, A>;
    type Item = T;
    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            start: 0,
            storage: ManuallyDrop::new(self),
        }
    }
}

impl<T, A: Allocator> Iterator for IntoIter<T, A> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.start >= self.storage.len {
            return None;
        }
        let val = unsafe { core::ptr::read(self.storage.idx_to_ptr(self.start)) };
        self.start += 1;
        Some(val)
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let old_start = self.start;
        self.start = old_start.saturating_add(n).min(self.storage.len);
        let removed = old_start..self.start;
        for idx in removed {
            unsafe {
                let val = self.storage.idx_to_ptr(idx);
                val.drop_in_place();
            }
        }
        self.next()
    }
}
impl<T, A: Allocator> ExactSizeIterator for IntoIter<T, A> {
    fn len(&self) -> usize {
        self.storage.len - self.start
    }
}
impl<T, A: Allocator> DoubleEndedIterator for IntoIter<T, A> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.start >= self.storage.len {
            return None;
        }
        unsafe { Some(self.storage.pop_unchecked()) }
    }
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        let old_end = self.storage.len;
        self.storage.len = old_end.saturating_sub(n).max(self.start);
        let removed = self.storage.len..old_end;
        for idx in removed {
            unsafe {
                let val = self.storage.idx_to_ptr(idx);
                val.drop_in_place();
            }
        }
        self.next()
    }
}
impl<T, A: Allocator> Drop for IntoIter<T, A> {
    fn drop(&mut self) {
        // Drop all remaining elements
        _ = self.nth(usize::MAX);
        if size_of::<T>() == 0 {
            return;
        }
        unsafe {
            self.storage.alloc.deallocate(
                self.storage.buf.cast(),
                Array::<T>::layout_for_len(self.storage.buf.len()),
            );
        }
    }
}
