# netdiff

Compare files over a TCP connection.


## How it works

You run `netdiff` on two computers and give it a filename. One side acts as the
server and the other as the client so that a tcp connection can be established,
but other than that both programs go through the same steps.

Both sides compute the merkle tree of the file. Then, the root hash is sent
over the network. When a received hash doesn't match what was sent, the child
hashes are queued to be sent. This is repeated until every block difference has
been detected.

This reduces the number of exchanged hashes when compared to hashing every
block and linearly comparing them, but it requires the full merkle tree to be
calculated before exchanges can begin.


## Problems

**Messages are exchanged in plaintext**. I considered adding TLS, but I'm using it
via an SSH tunnel for now.

There's no network resilience, so if something goes wrong the program stops.
This is particularly problematic because the merkle tree isn't stored anywhere,
and it takes a while to compute for large files.


## Usage

```
USAGE:
    netdiff [FLAGS] [OPTIONS] [filename]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v, --verbose    increase verbosity

OPTIONS:
    -b, --block-size <BYTES>       chunk size in bytes [default: 1048576]
    -c, --client <ADDRESS:PORT>    destination network address and port
    -s, --server <ADDRESS:PORT>    listening network address and port

ARGS:
    <filename>    The filename to compare
```
