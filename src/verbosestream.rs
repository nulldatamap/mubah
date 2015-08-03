
struct VerboseStream {
  inner : TcpStream
}

impl Read for VerboseStream {
  fn read( &mut self, buf : &mut [u8] ) -> std::io::Result<usize> {
    let r = self.inner.read( buf );
    match r {
      Ok( ref a ) => println!("VerboseStream: read {:?} bytes.", a ),
      Err( ref f ) => println!("VerboseStream: while reading: {:?}", f )
    }

    r
  }
}

impl Write for VerboseStream {
  fn write( &mut self, buf: &[u8] ) -> std::io::Result<usize> {
    let r = self.inner.write( buf );
    match r {
      Ok( ref a ) => println!("VerboseStream: wrote {:?} bytes.", a ),
      Err( ref f ) => println!("VerboseStream: while writing: {:?}", f )
    }

    r
  }

  fn flush( &mut self ) -> std::io::Result<()> {
    let r = self.inner.flush();
    match r {
      Ok( _ ) => println!("VerboseStream: flushed." ),
      Err( ref f ) => println!("VerboseStream: while flushing: {:?}", f )
    }

    r
  }
}
