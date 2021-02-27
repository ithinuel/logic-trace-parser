use anyhow::Error;

struct PrintSink<T>(T);

impl<T: Iterator<Item = (f64, Result<Box<dyn std::fmt::Debug>, Error>)>> Iterator for PrintSink<T> {
    type Item = ();
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(ts, res)| {
            match res {
                Ok(debug) => println!("{:.9}: {:?}", ts, debug),
                Err(e) => println!("{:.9}: {:?}", ts, e),
            };
        })
    }
}
