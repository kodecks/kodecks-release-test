fn main() {
    let current_dir = std::env::current_dir().unwrap();
    panic!("Current directory: {:?}", current_dir);
}
