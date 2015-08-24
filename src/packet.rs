use super::packets_capnp;
use cgmath::FixedArray;
use super::entity::{Hero, Entity, Hitbox, Pos2, Vec2};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::net::{TcpListener, TcpStream, UdpSocket, SocketAddr, ToSocketAddrs};
use std::io::{BufReader, BufWriter, Read, Write};
use capnp::message::Builder;
use capnp;
use udpstream::{UdpStream, BufUdpStream};

fn pos2_from_fixed( v : [f32; 2] ) -> Pos2 {
  Pos2::new( v[0], v[1] )
}

fn vec2_from_fixed( v : [f32; 2] ) -> Vec2 {
  Vec2::new( v[0], v[1] )
}

#[derive(Clone)]
pub struct InstructionPacket {
  pub hero_id : usize,
  pub move_to : Option<Pos2>
}

impl InstructionPacket {
  pub fn new( id : usize ) -> InstructionPacket {
    InstructionPacket { hero_id : id
                      , move_to : None }
  }
}

pub type SyncFrame = Hero;

#[derive(Clone)]
pub struct SyncPacket {
  pub hero_id    : usize,
  pub sync_frame : SyncFrame
}

impl SyncPacket {
  pub fn new( id : usize, sf : SyncFrame ) -> SyncPacket {
    SyncPacket { hero_id   : id
               , sync_frame: sf}
  }
} 

#[derive(Clone)]
pub enum Packet {
  InstructionPacket( InstructionPacket ),
  SyncPacket( SyncPacket ),
  Ping,
  YourPing( u32 )
}

type Stream<'a> = BufReader<&'a mut UdpStream>;

impl Packet {
  pub fn read_connect( stream : &mut UdpStream ) -> capnp::Result<()> {
    use capnp::serialize_packed;
    use capnp::message::ReaderOptions;

    let mut buffered_stream = BufReader::new( stream );

    let message_reader
      = try!( serialize_packed::read_message( &mut buffered_stream
                                            , ReaderOptions::new() ) );

    try!( message_reader.get_root::<packets_capnp::connect::Reader>() );

    Ok( () )
  }

  pub fn read_intial_sync( stream : &mut UdpStream )
    -> capnp::Result<usize> {
    use capnp::serialize_packed;
    use capnp::message::ReaderOptions;

    let mut buffered_stream = BufReader::new( stream );

    let message_reader
      = try!( serialize_packed::read_message( &mut buffered_stream
                                            , ReaderOptions::new() ) );

    let initial_sync
      = try!(
          message_reader.get_root::<packets_capnp::initial_sync::Reader>() );

    Ok( initial_sync.get_your_id() as usize )
  }

  pub fn read_packet( stream : &mut UdpStream ) -> capnp::Result<Packet> {
    use capnp::serialize_packed;
    use capnp::message::ReaderOptions;

    let mut buffered_stream = BufReader::new( stream );

    let message_reader
      = try!( serialize_packed::read_message( &mut buffered_stream
                                            , ReaderOptions::new() ) );

    let rpacket
      = try!( message_reader.get_root::<packets_capnp::packet::Reader>() );

    Ok( match try!( rpacket.which() ) {

      packets_capnp::packet::Which::Instruction( inst ) =>
        Packet::InstructionPacket(
            try!( Packet::read_instruction( try!( inst ) ) ) ),

      packets_capnp::packet::Which::Sync( sync ) =>
        Packet::SyncPacket( try!( Packet::read_sync( try!( sync ) ) ) ),

      packets_capnp::packet::Which::Ping( () ) => Packet::Ping,

      packets_capnp::packet::Which::YourPing( yp ) => Packet::YourPing( yp )
    } )

  } 

  pub fn read_instruction( inst : packets_capnp::instruction::Reader )
    -> capnp::Result<InstructionPacket> {
    
    let move_to = match try!( inst.borrow().get_move_to().which() ) {
      packets_capnp::instruction::move_to::Nowhere( v ) =>
        None,
      packets_capnp::instruction::move_to::Target( t ) =>
        Some( pos2_from_fixed( Packet::read_vec2( try!( t ) ) ) )
    };

    Ok( InstructionPacket { hero_id: inst.get_hero_id() as usize
                          , move_to: move_to } )
  }

  pub fn read_vec2( vec : packets_capnp::vec2::Reader ) -> [f32; 2] {
    [ vec.borrow().get_x(), vec.get_y() ]
  }

  pub fn read_sync( sync : packets_capnp::sync::Reader )
    -> capnp::Result<SyncPacket> {

    Ok(
    SyncPacket { hero_id: sync.borrow().get_hero_id() as usize
               , sync_frame:
                 try!( Packet::read_hero( try!( sync.get_sync_frame() ) ) ) } )
  }

  pub fn read_hero( hero : packets_capnp::hero::Reader )
    -> capnp::Result<Hero> {

    let target_pos = match try!( hero.borrow().get_target_pos().which() ) {
      packets_capnp::hero::target_pos::Nowhere( v ) =>
        None,
      packets_capnp::hero::target_pos::Target( t ) =>
        Some( pos2_from_fixed( Packet::read_vec2( try!( t ) ) ) )
    };

    Ok(
    Hero { entity    :
           try!( Packet::read_entity( try!( hero.borrow().get_entity() ) ) )
         , color     :
           Packet::read_color( try!( hero.borrow().get_color() ) )
         , target_pos: target_pos } )
  }

  pub fn read_entity( sync : packets_capnp::entity::Reader )
    -> capnp::Result<Entity> {
    Ok(
    Entity { pos:
             pos2_from_fixed(
                Packet::read_vec2( try!( sync.borrow().get_pos() ) ) )
           , vel:
             vec2_from_fixed(
                Packet::read_vec2( try!( sync.borrow().get_vel() ) ) )
           , hitbox:
             try!( Packet::read_hitbox( try!( sync.get_hitbox() ) ) ) } )
  }

  pub fn read_hitbox( hitbox : packets_capnp::hitbox::Reader )
    -> capnp::Result<Hitbox> {
    Ok( match try!( hitbox.which() ) {
      packets_capnp::hitbox::None( () ) => Hitbox::None,
      packets_capnp::hitbox::Circle( r ) => Hitbox::Circle( r )
    } )
  }

  pub fn read_color( color : packets_capnp::color::Reader ) -> [f32; 4] {
    [ color.get_r(), color.get_g(), color.get_b(), color.get_a() ]
  }

  pub fn write_connect( stream : &mut BufUdpStream ) {
    use capnp::serialize_packed;
    use packets_capnp::connect;

    let mut message = Builder::new_default();

    {
      let mut is = message.init_root::<connect::Builder>();
    }

    serialize_packed::write_message( stream, &mut message ).unwrap();
    stream.flush();
  }

  pub fn write_initial_sync( id : usize, stream : &mut BufUdpStream ) {
    use capnp::serialize_packed;
    use packets_capnp::initial_sync;

    let mut message = Builder::new_default();

    {
      let mut is = message.init_root::<initial_sync::Builder>();

      is.set_your_id( id as u8 );
    }

    serialize_packed::write_message( stream, &mut message ).unwrap();
    stream.flush();
  }

  pub fn write_packet( self, stream : &mut BufUdpStream ) {
    use capnp::serialize_packed;
    use packets_capnp::packet;

    println!( "{}", match &self { &Packet::InstructionPacket( .. ) => "inst"
                                , &Packet::SyncPacket( .. ) => "sync"
                                , _ => "other" } );

    let mut message = Builder::new_default();
    {
      let mut pkt = message.init_root::<packet::Builder>();

      match self {
        Packet::InstructionPacket( ip ) =>
          Packet::write_instruction( ip, pkt.init_instruction() ),

        Packet::SyncPacket( sp ) =>
          Packet::write_sync( sp, pkt.init_sync() ),

        Packet::Ping => pkt.set_ping( () ),
        Packet::YourPing( yp ) => pkt.set_your_ping( yp )
      }
    }

    serialize_packed::write_message( stream, &mut message ).unwrap();
    stream.flush();
  }


  pub fn write_instruction( ip   : InstructionPacket
                      , mut inst : packets_capnp::instruction::Builder ) {
    inst.set_hero_id( ip.hero_id as u8 );
    let mut move_to = inst.init_move_to();

    match ip.move_to {
      Some( v ) => Packet::write_vec2( v.into_fixed(), move_to.init_target() ),
      None => move_to.set_nowhere( () )
    }

  }

  pub fn write_vec2( v : [f32; 2], mut ve : packets_capnp::vec2::Builder ) {
    ve.set_x( v[0] );
    ve.set_y( v[1] );
  }

  pub fn write_sync( sp   : SyncPacket
               , mut sync : packets_capnp::sync::Builder ) {
    sync.set_hero_id( sp.hero_id as u8 );
    let mut frame = sync.init_sync_frame();

    Packet::write_hero( sp.sync_frame, frame );
  }

  pub fn write_hero( hero : Hero, mut frame : packets_capnp::hero::Builder ) {
    Packet::write_entity( hero.entity, frame.borrow().init_entity() );
    Packet::write_color( hero.color, frame.borrow().init_color() );
    let mut target_pos = frame.init_target_pos();

    match hero.target_pos {
      Some( v ) => Packet::write_vec2( v.into_fixed(), target_pos.init_target() ),
      None => target_pos.set_nowhere( () )
    }
  }

  pub fn write_entity( entity : Entity, mut ent : packets_capnp::entity::Builder ) {
    Packet::write_vec2( entity.pos.into_fixed(), ent.borrow().init_pos() );
    Packet::write_vec2( entity.vel.into_fixed(), ent.borrow().init_vel() );
    Packet::write_hitbox( entity.hitbox, ent.init_hitbox() );
  }

  pub fn write_hitbox( hitbox : Hitbox, mut hit : packets_capnp::hitbox::Builder ) {
    match hitbox {
      Hitbox::None => hit.set_none( () ),
      Hitbox::Circle( r ) => hit.set_circle( r )
    }
  }

  pub fn write_color( color : [f32; 4], mut col : packets_capnp::color::Builder ) {
    col.set_r( color[0] );
    col.set_g( color[1] );
    col.set_b( color[2] );
    col.set_a( color[3] );
  }

}

pub fn net_thread( mut stream : UdpStream
             , outbox     : Sender<Packet>
             , killer     : Receiver<()> ) {
  loop {
    outbox.send( Packet::read_packet( &mut stream ).unwrap() );
  }
}