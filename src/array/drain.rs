use super::Array;
use std::{alloc::Allocator, ops::RangeBounds};

impl<T, A: Allocator> Array<T, A> {
    pub fn drain<'d>(&'d mut self, range: impl RangeBounds<usize>) -> Drain<'d, T, A> {
        use std::ops::Bound;
        let start = match range.start_bound() {
            Bound::Unbounded => 0,
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n.saturating_add(1),
        };
        let end = match range.end_bound() {
            Bound::Unbounded => self.len(),
            Bound::Included(&n) => n.saturating_add(1),
            Bound::Excluded(&n) => n,
        };
        if start >= self.len() {
            panic!("Start index {start} would be out of bounds for Array of length {len}", len = self.len());
        }
        if end > self.len() {
            panic!("End index {end} would be out of bounds for Array of length {len}", len = self.len());
        }
        Drain {
            arr: self,
            hole_start: start,
            hole_end: end,
            start,
            end,
        }
    }
}

pub struct Drain<'d, T, A: Allocator> {
    arr: &'d mut Array<T, A>,
    hole_start: usize,
    hole_end: usize,
    start: usize,
    end: usize,
}

impl<'d, T, A: Allocator> Iterator for Drain<'d, T, A> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.start >= self.end {
            return None;
        }
        let val = unsafe { self.arr.idx_to_ptr(self.start).read() };
        self.start += 1;
        Some(val)
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        for i in (self.start..self.end).take(n) {
            self.start += 1;
            unsafe {
                self.arr.idx_to_ptr(i).drop_in_place();
            }
        }
        self.next()
    }
}

impl<'d, T, A: Allocator> DoubleEndedIterator for Drain<'d, T, A> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.start >= self.end {
            return None;
        }
        self.end += 1;
        let val = unsafe { self.arr.idx_to_ptr(self.end).read() };
        Some(val)
    }
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        for i in (self.start..self.end).rev().take(n) {
            self.end -= 1;
            unsafe {
                self.arr.idx_to_ptr(i).drop_in_place();
            }
        }
        self.next_back()
    }
}

impl<'d, T, A: Allocator> ExactSizeIterator for Drain<'d, T, A> {
    fn len(&self) -> usize {
        self.end - self.start
    }
}

impl<'d, T, A: Allocator> Drop for Drain<'d, T, A> {
    fn drop(&mut self) {
        // Need to Drop start..end
        let full_len = self.arr.len();
        unsafe {
            self.arr.set_len(self.hole_start);
            for i in self.start..self.end {
                self.arr.idx_to_ptr(i).drop_in_place();
            }
            let from = self.arr.idx_to_ptr(self.hole_end);
            let to = self.arr.idx_to_ptr(self.hole_start);
            let count = full_len - self.hole_end;
            from.copy_to(to, count);
            self.arr.set_len(self.hole_start + count);
        }
    }
}
