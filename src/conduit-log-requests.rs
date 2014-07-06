#![feature(globs)]
#![feature(phase)]

#[phase(plugin, link)] extern crate log;

extern crate time;
extern crate conduit;
extern crate middleware = "conduit-middleware";

use std::any::AnyRefExt;
use std::fmt::Show;

use conduit::{Request, Response};
use middleware::Middleware;

pub struct LogRequests(pub u32);

impl Middleware for LogRequests {
    fn before(&self, req: &mut Request) -> Result<(), Box<Show>> {
        req.mut_extensions().insert("conduit.log-requests.start",
                                    box time::precise_time_ns());
        Ok(())
    }

    fn after(&self, req: &mut Request,
             resp: Result<Response, Box<Show>>) -> Result<Response, Box<Show>> {
        let start = req.mut_extensions().pop(&"conduit.log-requests.start");
        let start = *start.unwrap().as_ref::<u64>().unwrap();

        match resp {
            Ok(ref resp) => self.log_message(req, start, resp.status.val0(),
                                             None),
            Err(ref e) => {
                let msg: &Show = *e;
                self.log_message(req, start, 500, Some(msg))
            }
        }
        resp
    }

}

impl LogRequests {
    fn log_message(&self, req: &mut Request, start: u64, status: uint,
                   msg: Option<&Show>) {
        let LogRequests(level) = *self;
        log!(level, "{} [{}] {} {} - {}ms {}{}",
             req.remote_ip(),
             time::now().rfc3339(),
             req.method(),
             req.path(),
             (time::precise_time_ns() - start) / 1000000,
             status,
             match msg {
                 None => String::new(),
                 Some(s) => format!(": {}", s),
             })
    }
}

#[cfg(test)]
mod tests {
    extern crate test = "conduit-test";

    use super::*;
    use std;
    use log;
    use conduit;
    use middleware;

    use std::io::{ChanWriter, ChanReader};
    use log::{Logger, LogRecord};
    use conduit::{Request, Response, Handler};

    struct MyWriter(ChanWriter);

    impl Logger for MyWriter {
        fn log(&mut self, record: &LogRecord) {
            let MyWriter(ref mut inner) = *self;
            (write!(inner, "{}", record.args)).unwrap();
        }
    }

    #[test]
    fn test_log() {
        let (sender, receiver) = channel();
        let mut reader = ChanReader::new(receiver);

        let mut builder = middleware::MiddlewareBuilder::new(handler);
        builder.add(LogRequests(log::ERROR));

        task(builder, sender);

        let result = reader.read_to_str().ok().expect("No response");
        let parts = result.as_slice().split(' ').map(|s| s.to_str()).collect::<Vec<String>>();

        assert_eq!(parts.get(0).as_slice(), "127.0.0.1");
        assert!(parts.get(1).as_slice().len() == "[2014-07-01T22:34:06-07:00]".len());
        assert_eq!(parts.get(2).as_slice(), "Get");
        assert_eq!(parts.get(3).as_slice(), "/foo");
    }

    fn task<H: Handler + 'static + Send>(handler: H, sender: Sender<Vec<u8>>) {
        spawn(proc() {
            log::set_logger(box MyWriter(ChanWriter::new(sender)));
            let mut request = test::MockRequest::new(conduit::Get, "/foo");
            let _ = handler.call(&mut request);
        });
    }

    fn handler(_: &mut Request) -> Result<Response, ()> {
        Ok(Response {
            status: (200, "OK"),
            headers: std::collections::HashMap::new(),
            body: box std::io::util::NullReader
        })
    }
}
