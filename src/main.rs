#[tokio::main]
async fn main() {
    //启用日志输出，你也可以使用其他日志框架，这个不限定的
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .filter_module(env!("CARGO_CRATE_NAME"), log::LevelFilter::Trace)
        .init();

    let b = B;
    b.a();
}

trait A {
    fn a(&self) {
        println!("a");
    }
}

struct B;

impl A for B {
    fn a(&self) {
        A::a(self);
        println!("b");
    }
}