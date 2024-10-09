fn main() {
    build_info_build::build_script();
    
    let current_dir = std::env::current_dir().unwrap();
    panic!("Current directory: {:?}", current_dir);
}
