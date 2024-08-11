mod asio_stream;
mod pa0;
mod symrs;
mod tests;

#[tokio::main]
async fn main() {
    pa0::pa0();
}
