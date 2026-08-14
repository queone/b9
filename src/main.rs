const PROGRAM_VERSION: &str = "0.1.0";

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--version") && args.next().is_none() {
        println!("b9 {PROGRAM_VERSION}");
        return;
    }

    println!("Hello, world!");
}
