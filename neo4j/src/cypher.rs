use std::collections::{HashMap, VecDeque};

use bolt::{BoltStream, BoltSummary};

use packstream::values::{Value, Data};

const USER_AGENT: &'static str = "rusty-bolt/0.1.0";

pub type Result<T> = ::bolt::Result<T>;

pub struct CypherStream {
    bolt: BoltStream,
    server_version: Option<String>,
    bookmark: Option<String>,
}
impl CypherStream {
    pub fn connect(address: &str, user: &str, password: &str) -> self::Result<CypherStream> {
        info!("Connecting to bolt://{} as {}", address, user);
        match BoltStream::connect(address) {
            Ok(mut bolt) => {
                bolt.pack_init(USER_AGENT, user, password);
                let init = bolt.collect_response();
                bolt.send();
                let init_summary = bolt.fetch_summary(init);
                let summary = init_summary.unwrap();
                bolt.compact_responses();

                let server_version = match summary {
                    BoltSummary::Success(ref metadata) => match metadata.get("server") {
                        Some(&Value::String(ref string)) => Some(string.clone()),
                        _ => None,
                    },
                    BoltSummary::Ignored(_) => panic!("Protocol violation! INIT should not be IGNORED"),
                    BoltSummary::Failure(_) => panic!("INIT returned FAILURE"),
                };

                info!("Connected to server version {:?}", server_version);
                Ok(CypherStream {
                    bolt: bolt,
                    server_version: server_version,
                    bookmark: None,
                })
            },
            Err(e) => Err(e)
        }
    }

    pub fn protocol_version(&self) -> u32 {
        self.bolt.protocol_version()
    }

    pub fn server_version(&self) -> &str {
        match self.server_version {
            Some(ref version) => &version[..],
            None => "",
        }
    }

    pub fn bookmark(&self) -> &Option<String> {
        &self.bookmark
    }

    pub fn set_bookmark(&mut self, bookmark: String) {
        self.bookmark = Some(bookmark);
    }

    pub fn clear_bookmark(&mut self) {
        self.bookmark = None;
    }

    pub fn begin_transaction(&mut self, bookmark: Option<&str>) {
        info!("BEGIN {:?}->|...|", bookmark);
        self.bolt.pack_run("BEGIN", match bookmark {
            Some(string) => parameters!("bookmark" => string),
            _ => parameters!(),
        });
        self.bolt.pack_discard_all();
        self.bolt.ignore_response();
        self.bolt.ignore_response();
    }

    pub fn commit_transaction(&mut self) {
        self.bolt.pack_run("COMMIT", parameters!());
        self.bolt.pack_discard_all();
        self.bolt.ignore_response();
        let body = self.bolt.collect_response();
        self.bolt.send();
        let summary = self.bolt.fetch_summary(body);
        self.bolt.compact_responses();

        let bookmark: Option<String> = match summary {
            Some(BoltSummary::Success(ref metadata)) => match metadata.get("bookmark") {
                Some(&Value::String(ref bookmark)) => Some(bookmark.clone()),
                _ => None,
            },
            _ => None,
        };

        info!("COMMIT |...|->{:?}", bookmark);
        self.bookmark = bookmark;
    }

    pub fn rollback_transaction(&mut self) {
        self.bolt.pack_run("ROLLBACK", parameters!());
        self.bolt.pack_discard_all();
        self.bolt.ignore_response();
        let body = self.bolt.collect_response();
        self.bolt.send();
        self.bolt.fetch_summary(body);
        self.bolt.compact_responses();
    }

    pub fn reset(&mut self) {
        self.bolt.pack_reset();
        let reset = self.bolt.collect_response();
        self.bolt.send();
        self.bolt.fetch_summary(reset);
        self.bolt.compact_responses();
    }

    pub fn run(&mut self, statement: &str, parameters: HashMap<&str, Value>) -> StatementResult {
        self.bolt.pack_run(statement, parameters);
        self.bolt.pack_pull_all();
        let head = self.bolt.collect_response();
        let body = self.bolt.collect_response();
        self.send();
        match self.fetch_header(head) {
            Some(header) => match header {
                BoltSummary::Success(metadata) => StatementResult { header: metadata, body: body },
                _ => panic!("Failed! Not successful."),
            },
            _ => panic!("Failed! No header summary"),
        }
    }

    fn send(&mut self) {
        self.bolt.send();
    }

    /// Fetch the RUN summary
    fn fetch_header(&mut self, response_id: usize) -> Option<BoltSummary> {
        let summary = self.bolt.fetch_summary(response_id);
        info!("HEADER {:?}", summary);
        self.bolt.compact_responses();
        match summary {
            Some(BoltSummary::Ignored(_)) => panic!("RUN was IGNORED"),
            Some(BoltSummary::Failure(_)) => {
                self.bolt.pack_ack_failure();
                self.bolt.ignore_response();
                self.bolt.send();
            },
            _ => (),
        };
        summary
    }

    /// Fetch the result detail
    pub fn fetch(&mut self, result: &StatementResult, into: &mut VecDeque<Data>) -> usize {
        self.bolt.fetch_detail(result.body, into)
    }

    /// Fetch the result summary
    pub fn fetch_summary(&mut self, result: &StatementResult) -> Option<BoltSummary> {
        let summary = self.bolt.fetch_summary(result.body);
        info!("SUMMARY {:?}", summary);
        self.bolt.compact_responses();
        match summary {
            Some(BoltSummary::Failure(_)) => {
                self.bolt.pack_ack_failure();
                self.bolt.ignore_response();
                self.bolt.send();
            },
            _ => (),
        };
        summary
    }
}

//#[derive(Copy, Clone)]
pub struct StatementResult {
    header: HashMap<String, Value>,
    body: usize,
}
impl StatementResult {
    pub fn keys(&self) -> &Value {
        self.header.get("fields").unwrap()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
