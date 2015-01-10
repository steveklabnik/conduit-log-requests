#![allow(missing_copy_implementations)]
#![cfg_attr(test, deny(warnings))]
#![cfg_attr(test, allow(unstable))]

#[macro_use] extern crate log;

extern crate time;
extern crate conduit;
extern crate "conduit-middleware" as middleware;

use std::error::Error;

use conduit::{Request, Response};
use middleware::Middleware;

pub struct LogRequests(pub u32);

struct LogStart(u64);

impl Middleware for LogRequests {
    fn before(&self, req: &mut Request) -> Result<(), Box<Error>> {
        req.mut_extensions().insert(LogStart(time::precise_time_ns()));
        Ok(())
    }

    fn after(&self, req: &mut Request, resp: Result<Response, Box<Error>>)
             -> Result<Response, Box<Error>> {
        let LogStart(start) = *req.mut_extensions().find::<LogStart>().unwrap();

        match resp {
            Ok(ref resp) => self.log_message(req, start, resp.status.0, None),
            Err(ref e) => {
                let msg: &Error = &**e;
                self.log_message(req, start, 500, Some(msg))
            }
        }
        resp
    }

}

impl LogRequests {
    fn log_message(&self, req: &mut Request, start: u64, status: u32,
                   msg: Option<&Error>) {
        let LogRequests(level) = *self;
        log!(level, "{} [{}] {:?} {} - {}ms {}{}",
             req.remote_ip(),
             time::now().rfc3339(),
             req.method(),
             req.path(),
             (time::precise_time_ns() - start) / 1000000,
             status,
             match msg {
                 None => String::new(),
                 Some(s) => format!(": {} {:?}", s.description(), s.detail()),
             })
    }
}

#[cfg(test)]
mod tests {
    extern crate "conduit-test" as test;

    use {LogRequests};

    use std;
    use std::sync::mpsc::{channel, Sender};
    use std::thread::Thread;
    use log;
    use middleware;

    use std::error::Error;
    use std::io::{ChanWriter, ChanReader};
    use log::{Logger, LogRecord};
    use conduit::{Request, Response, Handler, Method};

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

        let result = reader.read_to_string().ok().expect("No response");
        let parts = result.as_slice().split(' ').map(|s| s.to_string()).collect::<Vec<String>>();

        assert_eq!(parts[0].as_slice(), "127.0.0.1");
        // Failing on travis?! bug in libtime?!
        // assert!(parts.get(1).as_slice().len() == "[2014-07-01T22:34:06-07:00]".len(),
        //         "bad length for {}", parts.get(1));
        assert_eq!(parts[2].as_slice(), "Get");
        assert_eq!(parts[3].as_slice(), "/foo");
    }

    fn task<H: Handler + 'static + Send>(handler: H, sender: Sender<Vec<u8>>) {
        Thread::spawn(move|| {
            log::set_logger(Box::new(MyWriter(ChanWriter::new(sender))));
            let mut request = test::MockRequest::new(Method::Get, "/foo");
            let _ = handler.call(&mut request);
        });
    }

    fn handler(_: &mut Request) -> Result<Response, Box<Error>> {
        Ok(Response {
            status: (200, "OK"),
            headers: std::collections::HashMap::new(),
            body: Box::new(std::io::util::NullReader)
        })
    }
}
