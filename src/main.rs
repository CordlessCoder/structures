use std::fmt::Debug;

use structures::array::Array;

#[derive(Debug)]
pub struct Loud<T: Debug>(pub T);
impl<T: Debug> Drop for Loud<T> {
    fn drop(&mut self) {
        eprintln!("Dropping {:?}", self.0);
    }
}

fn main() {
    let mut arr: Array<i32> = (0..20).collect();
    arr.pop();
    dbg!(&arr);
}
