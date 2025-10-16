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
    let mut arr = Array::new();
    for i in 0..20 {
        arr.push(Loud(i));
    }
    arr.drain(0..5);
    // arr.retain(|i| i.0 % 2 == 0);
    dbg!(&arr);
}
