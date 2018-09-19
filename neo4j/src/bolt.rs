use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt;
use std::io::prelude::*;
use std::net::{TcpStream, ToSocketAddrs};
use std::result;

use packstream::{Packer, Unpacker};
use packstream::values::{Value, Data};

const HANDSHAKE: [u8; 20] = [0x60, 0x60, 0xB0, 0x17,
                             0x00, 0x00, 0x00, 0x01,
                             0x00, 0x00, 0x00, 0x00,
                             0x00, 0x00, 0x00, 0x00,
                             0x00, 0x00, 0x00, 0x00];

const MAX_CHUNK_SIZE: usize = 0xFFFF;


#[derive(Debug)]
pub enum BoltError {
    Connect(&'static str),
    Handshake(&'static str),
}

impl fmt::Display for BoltError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BoltError::Connect(ref err) => write!(f, "Connect error: {}", err),
            BoltError::Handshake(ref err) => write!(f, "Handshake error: {}", err),
        }
    }
}

impl Error for BoltError {
    fn description(&self) -> &str {
        match *self {
            BoltError::Connect(ref err) => err,
            BoltError::Handshake(ref err) => err,
        }
    }
}

pub struct BoltStream {
    stream: TcpStream,
    packer: Packer,
    end_of_request_markers: VecDeque<usize>,
    unpacker: Unpacker,
    responses: VecDeque<BoltResponse>,
    responses_done: usize,
    current_response_index: usize,
    protocol_version: u32,
}

pub type Result<T> = result::Result<T, BoltError>;

impl BoltStream {
    pub fn connect<A: ToSocketAddrs>(address: A) -> Result<BoltStream> {
        match TcpStream::connect(address) {
            Ok(mut stream) => match stream.write(&HANDSHAKE) {
                Ok(_) => {
                    let mut buf = [0; 4];
                    match stream.read(&mut buf) {
                        Ok(_) => {
                            let protocol_version: u32 = (buf[0] as u32) << 24 |
                                                        (buf[1] as u32) << 16 |
                                                        (buf[2] as u32) << 8 |
                                                        (buf[3] as u32);
                            debug!("S: <VERSION {}>", protocol_version);
                            Ok(BoltStream {
                                stream: stream,
                                packer: Packer::new(),
                                end_of_request_markers: VecDeque::new(),
                                unpacker: Unpacker::new(),
                                responses: VecDeque::new(),
                                responses_done: 0,
                                current_response_index: 0,
                                protocol_version: protocol_version,
                            })
                        },
                        Err(_) => Err(BoltError::Handshake("Error on read")),
                    }
                },
                Err(_) => Err(BoltError::Handshake("Error on write")),
            },
            Err(_) => Err(BoltError::Connect("Error on connect")),
        }
    }

    pub fn protocol_version(&self) -> u32 {
        self.protocol_version
    }

    fn mark_end_of_request(&mut self) {
        self.end_of_request_markers.push_back(self.packer.len());
    }

    /// Pack an INIT message.
    ///
    pub fn pack_init(&mut self, user_agent: &str, user: &str, password: &str) {
        debug!("C: INIT {:?} {{\"scheme\": \"basic\", \"principal\": {:?}, \"credentials\": \"...\"}}", user_agent, user);
        self.packer.pack_structure_header(2, 0x01);
        self.packer.pack_string(user_agent);
        self.packer.pack_map_header(3);
        self.packer.pack_string("scheme");
        self.packer.pack_string("basic");
        self.packer.pack_string("principal");
        self.packer.pack_string(user);
        self.packer.pack_string("credentials");
        self.packer.pack_string(password);
        self.mark_end_of_request();
    }

    /// Pack an ACK_FAILURE message.
    ///
    pub fn pack_ack_failure(&mut self) {
        debug!("C: ACK_FAILURE");
        self.packer.pack_structure_header(0, 0x0E);
        self.mark_end_of_request();
    }

    /// Pack a RESET message.
    ///
    pub fn pack_reset(&mut self) {
        debug!("C: RESET");
        self.packer.pack_structure_header(0, 0x0F);
        self.mark_end_of_request();
    }

    /// Pack a RUN message.
    ///
    pub fn pack_run(&mut self, statement: &str, parameters: HashMap<&str, Value>) {
        debug!("C: RUN {:?} {:?}", statement, parameters);
        self.packer.pack_structure_header(2, 0x10);
        self.packer.pack_string(statement);
        self.packer.pack_map_header(parameters.len());
        for (name, value) in &parameters {
            self.packer.pack_string(name);
            self.packer.pack(value);
        }
        self.mark_end_of_request();
    }

    /// Pack a DISCARD_ALL message.
    ///
    pub fn pack_discard_all(&mut self) {
        debug!("C: DISCARD_ALL");
        self.packer.pack_structure_header(0, 0x2F);
        self.mark_end_of_request();
    }

    /// Pack a PULL_ALL message.
    ///
    pub fn pack_pull_all(&mut self) {
        debug!("C: PULL_ALL");
        self.packer.pack_structure_header(0, 0x3F);
        self.mark_end_of_request();
    }

    /// Send all queued outgoing messages.
    ///
    pub fn send(&mut self) {
        debug!("C: <SEND>");
        let mut offset: usize = 0;
        for &mark in &self.end_of_request_markers {
            for chunk_data in self.packer[offset..mark].chunks(MAX_CHUNK_SIZE) {
                let chunk_size = chunk_data.len();
                let _ = self.stream.write(&[(chunk_size >> 8) as u8, chunk_size as u8]).unwrap();
                let _ = self.stream.write(&chunk_data).unwrap();
            }
            let _ = self.stream.write(&[0, 0]).unwrap();
            offset = mark;
        }
        self.packer.clear();
        self.end_of_request_markers.clear();
    }

    pub fn collect_response(&mut self) -> usize {
        self.responses.push_back(BoltResponse::new());
        self.responses_done + self.responses.len() - 1
    }

    pub fn ignore_response(&mut self) {
        self.responses.push_back(BoltResponse::done());
    }

    pub fn compact_responses(&mut self) {
        let mut pruning = true;
        while pruning && self.current_response_index > 0 {
            match self.responses.front() {
                Some(response) => {
                    if !response.done {
                        pruning = false;
                    }
                },
                _ => {
                    pruning = false;
                },
            }
            if pruning {
                self.responses.pop_front();
                self.responses_done += 1;
                self.current_response_index -= 1;
            }
        }
    }

    /// Fetches the next response message for the designated response,
    /// assuming that response is not already completely buffered.
    ///
    pub fn fetch_detail(&mut self, response_id: usize, into: &mut VecDeque<Data>) -> usize {
        let response_index = response_id - self.responses_done;
        while self.current_response_index < response_index {
            self.fetch();
        }
        if self.current_response_index == response_index {
            self.fetch();
        }
        match self.responses[response_index].detail.pop_front() {
            Some(data) => {
                into.push_back(data);
                1
            },
            _ => 0,
        }
    }

    /// Fetches all response messages for the designated response,
    /// assuming that response is not already completely buffered.
    ///
    pub fn fetch_summary(&mut self, response_id: usize) -> Option<BoltSummary> {
        let response_index = response_id - self.responses_done;
        while self.current_response_index <= response_index {
            self.fetch();
        }
        let ref mut response = self.responses[response_index];
        response.done = true;
        response.summary.take()
    }

    fn receive(&mut self) {
        self.unpacker.clear();
        let mut chunk_size: usize = self.read_chunk_size();
        while chunk_size > 0 {
            let _ = self.stream.read_exact(&mut self.unpacker.buffer(chunk_size));
            chunk_size = self.read_chunk_size();
        }
    }

    fn read_chunk_size(&mut self) -> usize {
        let chunk_header = &mut [0u8; 2];
        let _ = self.stream.read_exact(chunk_header);
//        log_line!("S: [{:02X} {:02X}]", chunk_header[0] as u8, chunk_header[1] as u8);
        0x100 * chunk_header[0] as usize + chunk_header[1] as usize
    }

    /// Reads the next message from the stream into the read buffer.
    ///
    fn fetch(&mut self) {
        self.receive();
        let response = self.responses.get_mut(self.current_response_index).unwrap();
        match self.unpacker.unpack_top() {
            Value::List(data) => {
                debug!("S: {:?}", data);
                response.detail.push_back(Data::Record(data));
            },
            Value::Message { signature, mut fields } => match signature {
                0x70 => {
                    self.current_response_index += 1;
                    match fields.len() {
                        0 => {
                            debug!("S: SUCCESS({{}})");
                            response.summary = Some(BoltSummary::Success(HashMap::new()));
                        },
                        _ => match fields.remove(0) {
                            Value::Dictionary(metadata) => {
                                debug!("S: SUCCESS({:?})", metadata);
                                response.summary = Some(BoltSummary::Success(metadata));
                            },
                            _ => panic!("Non-map metadata"),
                        },
                    }
                },
                0x7E => {
                    self.current_response_index += 1;
                    match fields.len() {
                        0 => {
                            debug!("S: IGNORED({{}})");
                            response.summary = Some(BoltSummary::Ignored(HashMap::new()));
                        },
                        _ => match fields.remove(0) {
                            Value::Dictionary(metadata) => {
                                debug!("S: IGNORED({:?})", metadata);
                                response.summary = Some(BoltSummary::Ignored(metadata));
                            },
                            _ => panic!("Non-map metadata"),
                        },
                    }
                },
                0x7F => {
                    self.current_response_index += 1;
                    match fields.len() {
                        0 => {
                            debug!("S: FAILURE({{}})");
                            response.summary = Some(BoltSummary::Failure(HashMap::new()));
                        },
                        _ => match fields.remove(0) {
                            Value::Dictionary(metadata) => {
                                debug!("S: FAILURE({:?})", metadata);
                                response.summary = Some(BoltSummary::Failure(metadata));
                            },
                            _ => panic!("Non-map metadata"),
                        },
                    }
                },
                _ => panic!("Unknown response message with signature {:02X}", signature),
            },
            _ => panic!("Response message is not a data or a summary"),
        }
    }

}

pub enum BoltSummary {
    Success(HashMap<String, Value>),
    Ignored(HashMap<String, Value>),
    Failure(HashMap<String, Value>),
}
impl fmt::Debug for BoltSummary {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BoltSummary::Success(ref values) => write!(f, "Success({:?})", values),
            BoltSummary::Ignored(ref values) => write!(f, "Ignored({:?})", values),
            BoltSummary::Failure(ref values) => write!(f, "Failure({:?})", values),
        }
    }
}

pub struct BoltResponse {
    detail: VecDeque<Data>,
    summary: Option<BoltSummary>,
    done: bool,
}
impl BoltResponse {
    pub fn new() -> BoltResponse {
        BoltResponse { detail: VecDeque::new(), summary: None, done: false }
    }
    pub fn done() -> BoltResponse {
        BoltResponse { detail: VecDeque::new(), summary: None, done: true }
    }
}

impl fmt::Debug for BoltResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.summary {
            Some(ref summary) => {
                match *summary {
                    BoltSummary::Success(ref metadata) => write!(f, "SUCCESS {:?}", metadata),
                    BoltSummary::Ignored(ref metadata) => write!(f, "IGNORED {:?}", metadata),
                    BoltSummary::Failure(ref metadata) => write!(f, "FAILURE {:?}", metadata),
                }
            },
            None => write!(f, "None"),
        }
    }
}
