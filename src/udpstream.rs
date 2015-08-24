use std::io::{BufReader, BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket, SocketAddr, ToSocketAddrs};
use std;

pub struct UdpStream {
  pub socket : UdpSocket,
  pub target : Option<SocketAddr>,
  pub sender : Option<SocketAddr>
}

impl UdpStream {
  pub fn new( sock : UdpSocket ) -> UdpStream {
    UdpStream { socket : sock, target : None, sender : None }
  }

  pub fn set_target( &mut self, target : SocketAddr ) {
    self.target = Some( target );
  }

  pub fn try_clone( &self ) -> std::io::Result<UdpStream> {
    Ok( UdpStream { socket: try!( self.socket.try_clone() )
                  , target: self.target.clone()
                  , sender: self.sender.clone()} )
  }
}

impl Read for UdpStream {
  fn read( &mut self, buf : &mut [u8] ) -> std::io::Result<usize> {
    match try!( self.socket.recv_from( buf ) ) {
      (r, a) => {
        self.sender = Some( a );
        Ok( r )
      }
    }
  }
}

impl Write for UdpStream {
  fn write( &mut self, buf : &[u8] ) -> std::io::Result<usize> {
    match self.target {
      Some( target ) => {
        self.socket.send_to( buf, target )
      },
      None => panic!( "No target selected for UdpStream::write" )
    }
  }

  fn flush( &mut self ) -> std::io::Result<()> {
    Ok( () )
  }
}

pub type BufUdpStream = BufWriter<UdpStream>;
