extern crate capnpc;

fn main() {
  ::capnpc::compile(".", &["packets.capnp"]).unwrap();
}
