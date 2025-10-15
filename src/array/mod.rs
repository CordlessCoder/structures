pub mod iter;
use std::{
    alloc::{Allocator, Global, Layout},
    fmt::Debug,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

pub struct Array<T, A: Allocator = Global> {
    len: usize,
    buf: NonNull<[MaybeUninit<T>]>,
    alloc: A,
}
// SAFETY: This impl tells the compiler that the Array type is okay to [Send] accross threads,
// if the type being stored is okay to send accross threads.
unsafe impl<T: Send> Send for Array<T> {}
// SAFETY: This impl tells the compiler that the Array type is okay to share accross threads,
// meaning Send around references to the Array, if that is allowed for the type being stored.
unsafe impl<T: Sync> Sync for Array<T> {}

impl<T> Array<T> {
    pub const fn new() -> Self {
        Array::new_in(Global)
    }
}

fn bytes_to_t<T>(bytes: NonNull<[u8]>) -> NonNull<[T]> {
    let new_len = bytes.len() / size_of::<T>();
    NonNull::slice_from_raw_parts(bytes.cast(), new_len)
}

impl<T, A: Allocator> Array<T, A> {
    pub const fn new_in(alloc: A) -> Self {
        let cap = if size_of::<T>() == 0 { usize::MAX } else { 0 };
        Array {
            len: 0,
            buf: NonNull::slice_from_raw_parts(NonNull::dangling(), cap),
            alloc,
        }
    }
    #[inline]
    fn layout_for_len(len: usize) -> Layout {
        Layout::array::<T>(len).unwrap()
    }
    unsafe fn grow_to_cap(&mut self, new_cap: usize) {
        let layout = Self::layout_for_len(new_cap);
        if self.buf.is_empty() {
            // Need to allocate a new buffer.
            let allocated = self.alloc.allocate(layout).unwrap();
            self.buf = bytes_to_t(allocated);
            return;
        }
        // Need to reallocate.
        unsafe {
            let new_buf = self
                .alloc
                .grow(
                    self.buf.cast(),
                    Self::layout_for_len(self.buf.len()),
                    Self::layout_for_len(new_cap),
                )
                .unwrap();
            self.buf = bytes_to_t(new_buf);
        }
    }
    /// Ensures at least `additional` elements can be inserted into the array without requiring a
    /// reallocation.
    pub fn reserve(&mut self, additional: usize) {
        let new_min = self.len + additional;
        if self.buf.len() >= new_min {
            // No reallocation necessary
            return;
        }
        let new_cap = new_min.next_power_of_two();
        unsafe {
            self.grow_to_cap(new_cap);
        }
    }
    pub fn push(&mut self, value: T) {
        self.reserve(1);
        unsafe {
            self.push_within_capacity_unchecked(value);
        }
    }
    pub fn push_within_capacity(&mut self, value: T) -> Result<(), T> {
        if self.len == self.buf.len() {
            return Err(value);
        }
        unsafe { self.push_within_capacity_unchecked(value) };
        Ok(())
    }
    /// Equivalent to [Self::push], but will not check that there is free capacity available.
    /// # Safety
    /// Requires that there is sufficient free capacity available.
    #[inline]
    pub unsafe fn push_within_capacity_unchecked(&mut self, value: T) {
        unsafe {
            let slot = self.idx_to_ptr(self.len);
            slot.write(value);
            self.len += 1;
        }
    }
    const fn idx_to_ptr(&self, idx: usize) -> *mut T {
        unsafe { self.buf.as_ptr().cast::<T>().add(idx) }
    }
    pub fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        unsafe { Some(self.pop_unchecked()) }
    }
    pub unsafe fn pop_unchecked(&mut self) -> T {
        unsafe {
            self.len = self.len.checked_sub(1).unwrap_unchecked();
            self.idx_to_ptr(self.len).read()
        }
    }
    pub fn remove(&mut self, idx: usize) -> Option<T> {
        if idx >= self.len {
            return None;
        }
        self.len -= 1;
        unsafe {
            let removed = self.idx_to_ptr(idx);
            let value = removed.read();
            core::ptr::copy(removed.add(1), removed, self.len - idx);
            Some(value)
        }
    }
    pub const fn len(&self) -> usize {
        self.len
    }
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn insert(&mut self, idx: usize, value: T) -> Result<(), T> {
        if idx > self.len {
            return Err(value);
        }
        self.reserve(1);
        self.len += 1;
        unsafe {
            let insert_at = self.idx_to_ptr(idx);
            core::ptr::copy(insert_at, insert_at.add(1), self.len - idx);
            insert_at.write(value);
        }
        Ok(())
    }
    pub fn swap(&mut self, a: usize, b: usize) {
        if a >= self.len {
            panic!(
                "Swap with index {a} is out of bounds for length {len}",
                len = self.len()
            );
        }
        if b >= self.len {
            panic!(
                "Swap with index {b} is out of bounds for length {len}",
                len = self.len()
            );
        }
        unsafe { self.idx_to_ptr(a).swap(self.idx_to_ptr(b)) };
    }
    /// A remove operation that, instead of preserving order, replaces the element with the last
    /// element in the list.
    /// This allows this operation to always run in O(1) time.
    pub fn swap_remove(&mut self, idx: usize) -> Option<T> {
        if idx >= self.len {
            return None;
        }
        let swap_with = self.idx_to_ptr(self.len - 1);
        let remove = self.idx_to_ptr(idx);
        unsafe {
            self.len -= 1;
            let value = remove.read();
            remove.copy_from_nonoverlapping(swap_with, 1);
            Some(value)
        }
    }
    pub fn shrink_to_fit(&mut self) {
        if size_of::<T>() == 0 {
            return;
        }
        if self.len == self.buf.len() {
            return;
        }
        unsafe {
            let ptr = self.alloc.shrink(
                self.buf.cast(),
                Self::layout_for_len(self.buf.len()),
                Self::layout_for_len(self.len),
            );
            self.buf = bytes_to_t(ptr.unwrap());
        }
    }
    const fn value_slice(&self) -> NonNull<[T]> {
        NonNull::slice_from_raw_parts(self.buf.cast(), self.len)
    }
    pub fn retain(&mut self, mut pred: impl FnMut(&mut T) -> bool) {
        struct DropGuard<'a, T, A: Allocator> {
            write: usize,
            read: usize,
            arr: &'a mut Array<T, A>,
        }
        impl<'a, T, A: Allocator> Drop for DropGuard<'a, T, A> {
            fn drop(&mut self) {
                unsafe {
                    while self.read < self.arr.len {
                        self.arr.idx_to_ptr(self.read).drop_in_place();
                        self.read += 1;
                    }
                }
                self.arr.len = self.write;
            }
        }
        let mut guard = DropGuard {
            write: 0,
            read: 0,
            arr: self,
        };
        while guard.read < guard.arr.len {
            unsafe {
                if !pred(guard.arr.get_unchecked_mut(guard.read)) {
                    guard.arr.idx_to_ptr(guard.read).drop_in_place();
                    guard.read += 1;
                    continue;
                }
                guard
                    .arr
                    .idx_to_ptr(guard.write)
                    .copy_from(guard.arr.idx_to_ptr(guard.read), 1);
                guard.write += 1;
            }
            guard.read += 1;
        }
        guard.arr.len = guard.write;
    }
}

impl<T: Debug, A: Allocator> Debug for Array<T, A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T, A: Allocator> Extend<T> for Array<T, A> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for value in iter {
            self.push(value);
        }
    }
}

impl<T> FromIterator<T> for Array<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut arr = Array::new();
        arr.extend(iter);
        arr
    }
}

impl<T, A: Allocator> Deref for Array<T, A> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        unsafe { self.value_slice().as_ref() }
    }
}

impl<T, A: Allocator> DerefMut for Array<T, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.value_slice().as_mut() }
    }
}

impl<T> Default for Array<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, A: Allocator> Drop for Array<T, A> {
    fn drop(&mut self) {
        let arr = core::mem::ManuallyDrop::new(self);
        unsafe {
            let copy: Array<T, A> = core::ptr::read(*arr.deref());
            // Defer to IntoIter's Drop impl
            _ = copy.into_iter();
        }
    }
}
