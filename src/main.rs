#[macro_use]
extern crate clap;

use std::io::Write;
use std::io::Read;
use std::io::Seek;
use std::hash::Hasher;
use clap::{App, Arg};

#[derive(Debug, Default)]
struct MerkleNode {
    offset: u64,
    hash: u64,
    children: Vec<usize>,
}

impl MerkleNode {
    fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

trait MerkleAsk {
    fn ask(&mut self, node: &MerkleNode) -> bool;
}

fn merklify(hashes: &mut Vec<MerkleNode>, start: usize, count: usize) {
    let step = 2;
    let mut inserted = 0;
    for i in (0..count).step_by(step) {
        let mut node = MerkleNode::default();

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for j in i .. (i + step).min(count) {
            hasher.write_u64(hashes[start + j].hash);
            node.children.push(start + j);
            node.offset = node.offset.min(hashes[start + j].offset);
        }

        node.hash = hasher.finish();
        hashes.push(node);
        inserted += 1;
    }

    if inserted > 1 {
        merklify(hashes, start + count, inserted);
    }
}

fn chunk_hashes(content: &mut dyn std::io::Read, block_size: u64) -> Vec<MerkleNode> {
    let mut hashes = vec![];
    let mut chunk = Vec::with_capacity(block_size as usize);

    loop {
        chunk.clear();
        match content.take(block_size).read_to_end(&mut chunk) {
            Err(_) | Ok(0) => {
                return hashes;
            },
            Ok(_) => {
                let mut node = MerkleNode::default();
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                hasher.write(&chunk);
                node.hash = hasher.finish();
                node.offset = block_size * hashes.len() as u64;
                hashes.push(node);
            }
        }
    }
}

fn merkle_tree(content: &mut dyn std::io::Read, block_size: u64) -> Vec<MerkleNode> {
    let mut hashes = chunk_hashes(content, block_size);

    let count = hashes.len();
    merklify(&mut hashes, 0, count);
    hashes
}

fn merkle_diff(tree: &Vec<MerkleNode>, asker: &mut dyn MerkleAsk) -> (Vec<usize>, usize) {
    let mut blocks = vec![];
    let mut questions = 0;
    let mut queue = std::collections::VecDeque::new();

    queue.push_back(tree.len() - 1);
    while !queue.is_empty() {
        let current = queue.pop_front().unwrap();

        for &idx in tree[current].children.iter() {
            questions += 1;
            if !asker.ask(&tree[idx]) {
                if tree[idx].is_leaf() {
                    blocks.push(idx);
                } else {
                    queue.push_back(idx);
                }
            }
        }

    }

    (blocks, questions)
}

struct NetworkAsker {
    conn: std::net::TcpStream,
}

impl MerkleAsk for NetworkAsker {
    fn ask(&mut self, node: &MerkleNode) -> bool {
        let mut answer = [0; 8];
        self.conn.write(&node.hash.to_le_bytes()).unwrap();
        self.conn.flush().unwrap();
        self.conn.read_exact(&mut answer).unwrap();

        u64::from_le_bytes(answer) == node.hash
    }
}

fn main() {
    let matches = App::new("netdiff")
        .version("0.2.0")
        .author("Hugo Peixoto <hugo.peixoto@gmail.com>")
        .about("Compare two files over the network")
        .arg(
            Arg::with_name("filename").index(1)
                .help("The filename to compare"),
        )
        .arg(
            Arg::with_name("server").short("s").long("server")
                .value_name("ADDRESS:PORT")
                .takes_value(true)
                .conflicts_with("client")
                .help("listening network address and port")
        )
        .arg(
            Arg::with_name("client").short("c").long("client")
                .value_name("ADDRESS:PORT")
                .takes_value(true)
                .conflicts_with("server")
                .help("destination network address and port"),
        )
        .arg(
            Arg::with_name("block_size").short("b").long("block-size")
                .value_name("BYTES")
                .takes_value(true)
                .default_value("1048576")
                .help("chunk size in bytes"),
        )
        .arg(
            Arg::with_name("verbose").short("v").long("verbose")
                .takes_value(false)
                .help("increase verbosity"),
        )
        .get_matches();

    let verbose = matches.is_present("verbose");

    let mut file = match matches.value_of("filename") {
        Some(filename) => {
            if verbose { println!("comparing {}", filename); }
            std::fs::File::open(filename).unwrap()
        },
        None => panic!("You must specify a filename"),
    };

    let conn = if let Some(address) = matches.value_of("server") {
        std::net::TcpListener::bind(address).unwrap().accept().unwrap().0
    } else if let Some(address) = matches.value_of("client") {
        std::net::TcpStream::connect(address).unwrap()
    } else {
        panic!("");
    };
    let mut asker = NetworkAsker { conn };

    let block_size = value_t!(matches, "block_size", u64).unwrap();

    if verbose { eprintln!("building tree...") };
    let tree = merkle_tree(&mut file, block_size);
    if verbose { eprintln!("done. ({} nodes)", tree.len()) };

    let mut total_exchanges = 0;
    let (blocks, exchanges) = merkle_diff(&tree, &mut asker);
    total_exchanges += exchanges;
    if verbose { eprintln!("block hash exchanges: {}", exchanges) };

    if !blocks.is_empty() {
        if verbose { eprintln!("mismatched blocks: {:?}", blocks); };

        for block in blocks {
            file.seek(std::io::SeekFrom::Start(tree[block].offset)).unwrap();

            let subtree = merkle_tree(&mut (&mut file).take(block_size), 1);

            let (bytes, exchanges) = merkle_diff(&subtree, &mut asker);
            total_exchanges += exchanges;

            for block_offset in bytes {
                let file_offset = tree[block].offset + block_offset as u64;
                let mut value = [0; 1];
                file.seek(std::io::SeekFrom::Start(file_offset)).unwrap();
                file.read_exact(&mut value).unwrap();
                println!("{}={:x?}", file_offset, value);
            }
        }

        if verbose { eprintln!("total exchanges: {}", total_exchanges); }
        std::process::exit(1);
    }
}
