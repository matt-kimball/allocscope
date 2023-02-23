use std::error::Error;

fn iter(_: usize) -> Vec<u8> {
    vec![0; 1024 * 1024]
}

fn main() -> Result<(), Box<dyn Error>> {
    for i in 0..1024 {
        iter(i);
    }

    Ok(())
}
