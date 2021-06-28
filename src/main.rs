use sha2::{Sha256, Digest};

use std::io::Write;
use std::io::Read;

#[derive(Debug)]
pub struct MerkleNode {
    hash: [u8; 32],
    left: Option<usize>,
    right: Option<usize>,
    depth: usize,
}

trait MerkleAsk {
    fn ask(&mut self, node: &MerkleNode) -> bool;
}

fn pretty_hash(hash: &[u8; 32]) -> String {
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

fn merklify(hashes: &mut Vec<MerkleNode>, start: usize, count: usize) {
    let mut inserted = 0;
    for i in (0..count).step_by(2) {
        let mut hash = [0; 32];
        let has_right = i + 1 < count;

        let mut hasher = Sha256::new();

        hasher.update(hashes[start + i].hash);
        if has_right {
            hasher.update(hashes[start + i + 1].hash);
        }

        hash.copy_from_slice(&hasher.finalize());
        hashes.push(MerkleNode {
            hash,
            left: Some(start + i),
            right: if has_right { Some(start + i + 1) } else { None },
            depth: hashes[start + i].depth + 1,
        });

        inserted += 1;
    }

    if inserted > 1 {
        merklify(hashes, start + count, inserted);
    }
}

fn merkle_tree(content: &mut dyn std::io::Read, block_size: u64) -> Vec<MerkleNode> {
    let mut hashes = vec![];

    loop {
        let mut chunk = Vec::with_capacity(block_size as usize);
        match content.take(block_size).read_to_end(&mut chunk) {
            Err(_) | Ok(0) => {
                let c = hashes.len();
                merklify(&mut hashes, 0, c);
                return hashes;
            },
            Ok(_) => {
                let mut hash: [u8; 32] = [0; 32];
                hash.copy_from_slice(&Sha256::digest(&chunk));
                hashes.push(MerkleNode { hash, left: None, right: None, depth: 0 });
            }
        }
    }
}

struct StdinAsker {
}

impl MerkleAsk for StdinAsker {
    fn ask(&mut self, node: &MerkleNode) -> bool {
        println!("test {}", pretty_hash(&node.hash));
        print!("match? ");
        std::io::stdout().flush().unwrap();

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();

        input == "y\n"
    }
}

pub fn merkle_print(tree: &Vec<MerkleNode>, idx: usize, indent: usize) {
    println!("{:indent$}{} {}", "", tree[idx].depth, pretty_hash(&tree[idx].hash), indent=indent);

    if let Some(idx) = tree[idx].left {
        merkle_print(tree, idx, indent + 2);
    }
    if let Some(idx) = tree[idx].right {
        merkle_print(tree, idx, indent + 2);
    }
}

fn merkle_diff(tree: &Vec<MerkleNode>, asker: &mut dyn MerkleAsk) -> Vec<usize> {
    let mut blocks = vec![];
    let mut queue = std::collections::VecDeque::new();

    queue.push_back(tree.len() - 1);
    while !queue.is_empty() {
        let current = queue.pop_front().unwrap();
        if let Some(idx) = tree[current].left {
            if !asker.ask(&tree[idx]) {
                queue.push_back(idx);
            }
        }

        if let Some(idx) = tree[current].right {
            if !asker.ask(&tree[idx]) {
                queue.push_back(idx);
            }
        }

        if tree[current].left.is_none() && tree[current].right.is_none() {
            blocks.push(current)
        }
    }

    blocks
}

struct NetworkAsker {
    conn: std::net::TcpStream,
}

impl MerkleAsk for NetworkAsker {
    fn ask(&mut self, node: &MerkleNode) -> bool {
        let mut answer = [0; 32];
        self.conn.write(&node.hash).unwrap();
        self.conn.flush().unwrap();
        self.conn.read_exact(&mut answer).unwrap();

        // println!("sent {}, received {}", pretty_hash(&node.hash), pretty_hash(&answer));

        answer == node.hash
    }
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let filename = &args[1];
    let server = args[2] == "-s";
    let address = &args[3];

    println!("checking {}", filename);
    let mut f = std::fs::File::open(filename).unwrap();

    println!("building tree...");
    let tree = merkle_tree(&mut f, 1024*1024);
    // merkle_print(&tree, tree.len() - 1, 0);
    println!("done. ({} nodes)", tree.len());

    let conn = if server {
        std::net::TcpListener::bind(address).unwrap().accept().unwrap().0
    } else {
        std::net::TcpStream::connect(address).unwrap()
    };

    let blocks = merkle_diff(&tree, &mut NetworkAsker{ conn });

    if !blocks.is_empty() {
        println!("mismatched blocks:");
        for block in blocks {
            println!("{}", block);
        }
    }
}
