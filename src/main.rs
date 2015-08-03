extern crate graphics;
extern crate piston_window;
extern crate cgmath;
extern crate rand;
extern crate capnp;
extern crate time;

use piston_window::*;
use cgmath::{Vector2, Point2, Point, Vector, EuclideanVector, FixedArray};
use capnp::message::Builder;
use rand::{thread_rng, Rng};
use std::default::Default;
use std::net::{TcpListener, TcpStream, UdpSocket, SocketAddr, ToSocketAddrs};
use std::thread;
use std::io::{BufReader, BufWriter, Read, Write};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use time::{Timespec, get_time};

mod packets_capnp {
  include!( concat!( env!("OUT_DIR"), "/packets_capnp.rs" ) );
}

type Vec2 = cgmath::Vector2<f32>;
type Pos2 = cgmath::Point2<f32>;

const GAME_TITLE : &'static str = "Mubah - v0.1.0";

struct UdpStream {
  socket : UdpSocket,
  target : Option<SocketAddr>,
  sender : Option<SocketAddr>
}

impl UdpStream {
  fn new( sock : UdpSocket ) -> UdpStream {
    UdpStream { socket : sock, target : None, sender : None }
  }

  fn set_target( &mut self, target : SocketAddr ) {
    self.target = Some( target );
  }

  fn try_clone( &self ) -> std::io::Result<UdpStream> {
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

type BufUdpStream = BufWriter<UdpStream>;

#[derive(Clone)]
struct GameSettings {
  pub resolution : [u32; 2],
  pub fullscreen : bool,
  pub vsync      : bool,
  pub host       : Option<String>
}

impl GameSettings {
  pub fn make_window( &self ) -> PistonWindow {
    WindowSettings::new( GAME_TITLE, self.resolution )
      .exit_on_esc( true )
      .vsync( self.vsync )
      .fullscreen( self.fullscreen )
      .into()
  }

  pub fn make_net_controller( &self ) -> NetController {
    NetController::new( self.host.clone() )
  }
}

impl Default for GameSettings {
  fn default() -> GameSettings {
    GameSettings {
      resolution : [ 640, 480 ],
      fullscreen : false,
      vsync      : false,
      host       : None
    }
  }
}

fn pos2_from_fixed( v : [f32; 2] ) -> Pos2 {
  Pos2::new( v[0], v[1] )
}

fn vec2_from_fixed( v : [f32; 2] ) -> Vec2 {
  Vec2::new( v[0], v[1] )
}

#[derive(Clone)]
enum Hitbox {
  None,
  Circle( f32 )
}

#[derive(Clone)]
struct Entity {
  pos    : Pos2,
  vel    : Vec2,
  hitbox : Hitbox
}

impl Entity {
  fn update( &mut self, delta_time : f64 ) {
    self.pos = self.pos.add_v( &self.vel.mul_s( delta_time as f32 ) );

    self.vel = Vec2::new( 0.0, 0.0 );
  }
}

#[derive(Clone)]
struct Hero {
  entity     : Entity,
  color      : [f32; 4],
  target_pos : Option<Pos2>
}

impl Hero {
  fn new( sp : Pos2 ) -> Hero {
    let mut r = thread_rng();
    // Generate the color
    let c = [ r.gen_range( 0.0, 1.0 ), r.gen_range( 0.0, 1.0 )
            , r.gen_range( 0.0, 1.0 ), 1.0 ];

    Hero { entity     : Entity { pos   : sp
                               , vel   : Vec2::new( 0.0, 0.0 )
                               , hitbox: Hitbox::None }
         , color      : c
         , target_pos : None }
  }

  fn instruct( &mut self, instr : InstructionPacket ) {
    if let Some( p ) = instr.move_to {
      self.target_pos = Some( p );
    }
  }

  fn update( &mut self, delta_time : f64 ) {

    if let Some( dest ) = self.target_pos {
      if self.entity.pos.sub_p( &dest ).length() < 1.0 {
        self.entity.pos = dest;
        self.target_pos = None;
      } else {
        self.entity.vel = dest.sub_p( &self.entity.pos ).normalize_to( 100.0 );
      }
    }

    self.entity.update( delta_time );
  }

}

#[derive(Clone)]
struct InstructionPacket {
  hero_id : usize,
  move_to : Option<Pos2>
}

impl InstructionPacket {
  fn new( id : usize ) -> InstructionPacket {
    InstructionPacket { hero_id : id
                      , move_to : None }
  }
}

type SyncFrame = Hero;

#[derive(Clone)]
struct SyncPacket {
  hero_id    : usize,
  sync_frame : SyncFrame
}

impl SyncPacket {
  fn new( id : usize, sf : SyncFrame ) -> SyncPacket {
    SyncPacket { hero_id   : id
               , sync_frame: sf}
  }
} 

#[derive(Clone)]
enum Packet {
  InstructionPacket( InstructionPacket ),
  SyncPacket( SyncPacket ),
  Ping,
  YourPing( u32 )
}

type Stream<'a> = BufReader<&'a mut UdpStream>;

impl Packet {
  fn read_connect( stream : &mut UdpStream ) -> capnp::Result<()> {
    use capnp::serialize_packed;
    use capnp::message::ReaderOptions;

    let mut buffered_stream = BufReader::new( stream );

    let message_reader
      = try!( serialize_packed::read_message( &mut buffered_stream
                                            , ReaderOptions::new() ) );

    try!( message_reader.get_root::<packets_capnp::connect::Reader>() );

    Ok( () )
  }

  fn read_intial_sync( stream : &mut UdpStream )
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

  fn read_packet( stream : &mut UdpStream ) -> capnp::Result<Packet> {
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

  fn read_instruction( inst : packets_capnp::instruction::Reader )
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

  fn read_vec2( vec : packets_capnp::vec2::Reader ) -> [f32; 2] {
    [ vec.borrow().get_x(), vec.get_y() ]
  }

  fn read_sync( sync : packets_capnp::sync::Reader )
    -> capnp::Result<SyncPacket> {

    Ok(
    SyncPacket { hero_id: sync.borrow().get_hero_id() as usize
               , sync_frame:
                 try!( Packet::read_hero( try!( sync.get_sync_frame() ) ) ) } )
  }

  fn read_hero( hero : packets_capnp::hero::Reader )
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

  fn read_entity( sync : packets_capnp::entity::Reader )
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

  fn read_hitbox( hitbox : packets_capnp::hitbox::Reader )
    -> capnp::Result<Hitbox> {
    Ok( match try!( hitbox.which() ) {
      packets_capnp::hitbox::None( () ) => Hitbox::None,
      packets_capnp::hitbox::Circle( r ) => Hitbox::Circle( r )
    } )
  }

  fn read_color( color : packets_capnp::color::Reader ) -> [f32; 4] {
    [ color.get_r(), color.get_g(), color.get_b(), color.get_a() ]
  }

  fn write_connect( stream : &mut BufUdpStream ) {
    use capnp::serialize_packed;
    use packets_capnp::connect;

    let mut message = Builder::new_default();

    {
      let mut is = message.init_root::<connect::Builder>();
    }

    serialize_packed::write_message( stream, &mut message ).unwrap();
    stream.flush();
  }

  fn write_initial_sync( id : usize, stream : &mut BufUdpStream ) {
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

  fn write_packet( self, stream : &mut BufUdpStream ) {
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


  fn write_instruction( ip   : InstructionPacket
                      , mut inst : packets_capnp::instruction::Builder ) {
    inst.set_hero_id( ip.hero_id as u8 );
    let mut move_to = inst.init_move_to();

    match ip.move_to {
      Some( v ) => Packet::write_vec2( v.into_fixed(), move_to.init_target() ),
      None => move_to.set_nowhere( () )
    }

  }

  fn write_vec2( v : [f32; 2], mut ve : packets_capnp::vec2::Builder ) {
    ve.set_x( v[0] );
    ve.set_y( v[1] );
  }

  fn write_sync( sp   : SyncPacket
               , mut sync : packets_capnp::sync::Builder ) {
    sync.set_hero_id( sp.hero_id as u8 );
    let mut frame = sync.init_sync_frame();

    Packet::write_hero( sp.sync_frame, frame );
  }

  fn write_hero( hero : Hero, mut frame : packets_capnp::hero::Builder ) {
    Packet::write_entity( hero.entity, frame.borrow().init_entity() );
    Packet::write_color( hero.color, frame.borrow().init_color() );
    let mut target_pos = frame.init_target_pos();

    match hero.target_pos {
      Some( v ) => Packet::write_vec2( v.into_fixed(), target_pos.init_target() ),
      None => target_pos.set_nowhere( () )
    }
  }

  fn write_entity( entity : Entity, mut ent : packets_capnp::entity::Builder ) {
    Packet::write_vec2( entity.pos.into_fixed(), ent.borrow().init_pos() );
    Packet::write_vec2( entity.vel.into_fixed(), ent.borrow().init_vel() );
    Packet::write_hitbox( entity.hitbox, ent.init_hitbox() );
  }

  fn write_hitbox( hitbox : Hitbox, mut hit : packets_capnp::hitbox::Builder ) {
    match hitbox {
      Hitbox::None => hit.set_none( () ),
      Hitbox::Circle( r ) => hit.set_circle( r )
    }
  }

  fn write_color( color : [f32; 4], mut col : packets_capnp::color::Builder ) {
    col.set_r( color[0] );
    col.set_g( color[1] );
    col.set_b( color[2] );
    col.set_a( color[3] );
  }

}

fn net_thread( mut stream : UdpStream
             , outbox     : Sender<Packet>
             , killer     : Receiver<()> ) {
  loop {
    outbox.send( Packet::read_packet( &mut stream ).unwrap() );
  }
}

#[derive(Clone, PartialEq, Eq)]
enum PingStatus {
  Ready,
  Awaiting( Timespec )
}

struct NetController {
  net_thread_killer      : Sender<()>,
  net_thread_outbox      : Receiver<Packet>,
  output_stream          : BufUdpStream,
  packets                : Vec<Packet>,
  frames_since_last_sync : usize,
  assigned_hero_id       : usize,
  ping                   : u32,
  ping_status            : PingStatus
}

impl NetController {
  pub fn new( mut host : Option<String> ) -> NetController {
    let (inb, outb) = channel();
    let (killer, killed) = channel();

    // Try to connect
    let port = if host.is_some() {
      4004
    } else {
      4114
    };

    let socket = UdpSocket::bind( ("0.0.0.0", port) ).unwrap();
    let mut stream = BufWriter::new( UdpStream::new( socket ) );

    let mut id = 0;

    // Do client handshake procedure
    if let Some( h ) = host {
      let addr = (&h[..], 4114).to_socket_addrs().unwrap().next().unwrap();

      stream.get_mut().set_target( addr );
      Packet::write_connect( &mut stream );
      id = Packet::read_intial_sync( stream.get_mut() ).unwrap();
    } else {
      Packet::read_connect( stream.get_mut() ).unwrap();
      // TODO: Figure what what the client ID actually should be
      stream.get_mut().target = stream.get_ref().sender;
      Packet::write_initial_sync( 1, &mut stream );
    }

    let usstream = stream.get_ref().try_clone().unwrap();

    thread::spawn( move || {
      net_thread( usstream, inb, killed );
    } );

    NetController { net_thread_killer     : killer
                  , net_thread_outbox     : outb
                  , output_stream         : stream
                  , packets               : Vec::new()
                  , frames_since_last_sync: 420
                  , assigned_hero_id      : id
                  , ping                  : 0
                  , ping_status           : PingStatus::Ready }
  }

  pub fn poke_packets( &mut self ) -> bool {
    
    match self.net_thread_outbox.try_recv() {
      Ok( o ) => self.packets.push( o ),
      Err( TryRecvError::Disconnected ) =>
        panic!( "Disconnected from net thread." ),
      _ => {}
    }

    !self.packets.is_empty()
  }

  pub fn poke_sync( &mut self ) -> bool {
    self.frames_since_last_sync += 1;

    // Ping every 60 frames
    /* DISABLED PINING

    if self.frames_since_last_sync % 60 == 0
    && self.ping_status == PingStatus::Ready {
      // Ping your host
      Packet::Ping.write_packet( &mut self.output_stream );
      self.ping_status = PingStatus::Awaiting( get_time() );
    }

    */

    self.frames_since_last_sync >= 120
  }

  pub fn send_sync_packet( &mut self, sp : SyncPacket ) {
    self.frames_since_last_sync = 0;
    Packet::SyncPacket( sp ).write_packet( &mut self.output_stream );
  }

  pub fn send_instruction( &mut self, ip : InstructionPacket ) {
    Packet::InstructionPacket( ip ).write_packet( &mut self.output_stream );
  }

  pub fn handle_ping( &mut self ) {
    match self.ping_status {
      PingStatus::Ready => Packet::Ping.write_packet( &mut self.output_stream ),
      PingStatus::Awaiting( ts ) => {
        let now = get_time();
        // Calcualte the difference in time, and convert it to milliseconds
        let ms = ( now.sec - ts.sec ) as u32 * 1000
               + ( now.nsec - ts.nsec ) as u32 / 1000000;
        // Send the ping
        Packet::YourPing( ms ).write_packet( &mut self.output_stream );
        self.ping_status = PingStatus::Ready;
      }
    }
  }

  pub fn update_ping( &mut self, p : u32 ) {
    self.ping = p;
    println!( "Ping: {}ms", p );
  }

}

impl Iterator for NetController {
  type Item = Packet;

  fn next( &mut self ) -> Option<Packet> {
    if !self.packets.is_empty() {
      self.packets
          .pop()
    } else {
      None
    }
  }
}

impl Drop for NetController {
  fn drop( &mut self ) {
    // Kill it
    self.net_thread_killer.send( () );
  }
}


#[derive(Clone)]
struct Controller {
  hero_id            : usize,
  dirty              : bool,
  instruction_packet : InstructionPacket
}

impl Controller {
  fn new( id : usize ) -> Controller {
    Controller { hero_id            : id
               , dirty              : false
               , instruction_packet : InstructionPacket::new( id ) }
  }

  fn refresh( &mut self ) {
    if !self.dirty {
      return
    } 

    self.instruction_packet = InstructionPacket::new( self.hero_id );
    self.dirty = false;
  }
}

const SPAWN_POINT : Pos2 = Pos2 { x : 100.0, y : 100.0 };
const OTHER_SPAWN_POINT : Pos2 = Pos2 { x : 200.0, y : 300.0 };

struct Game {
  net_controller    : NetController,
  controller        : Controller,
  heroes            : Vec<Hero>,
  cursor            : Pos2,
  debug             : bool
}

impl Game {
  fn new( nc : NetController ) -> Game {
    let id = nc.assigned_hero_id;

    Game { net_controller : nc
         , controller     : Controller::new( id )
         , heroes         : vec![ Hero::new( SPAWN_POINT )
                                , Hero::new( OTHER_SPAWN_POINT ) ]
         , cursor         : Pos2::new( 0.0, 0.0 )
         , debug          : false }
  }

  fn update_cursor( &mut self, x : f64, y : f64 ) {
    self.cursor = Pos2::new( x as f32, y as f32 );
  }

  fn input_press( &mut self, button : Button ) {

    if let Button::Keyboard( Key::D ) = button {
      self.debug = true;
    }

    self.controller.instruction_packet.move_to =
      Some( Pos2::new( self.cursor.x, self.cursor.y ) );

    self.controller.dirty = true;
  }

  fn instruct_hero( &mut self, ip : InstructionPacket ) {
    self.heroes[ip.hero_id].instruct( ip );
  }

  fn sync_hero( &mut self, sp : SyncPacket ) {
    self.heroes[sp.hero_id] = sp.sync_frame;
  }

  fn send_controlled_hero_sync( &mut self ) {
    let controlled_hero = self.heroes[self.controller.hero_id].clone();
    let sync_packet
      = SyncPacket::new( self.controller.hero_id, controlled_hero );
    self.net_controller.send_sync_packet( sync_packet );
  }

  fn update( &mut self, delta_time : f64 ) {
    // Send the instructions to the player's hero
    // TODO: fold together spammed instructions
    if self.controller.dirty {
      let mut ip = self.controller.instruction_packet.clone();
      self.instruct_hero( ip );
      ip = self.controller.instruction_packet.clone();
      self.net_controller.send_instruction( ip );
    }

    if self.net_controller.poke_packets() {
      loop {
        if let Some( u ) = self.net_controller.next() {
          match u {
            Packet::InstructionPacket( ip ) =>
              self.instruct_hero( ip ),
            Packet::SyncPacket( sp ) =>
              self.sync_hero( sp ),
            Packet::Ping => self.net_controller.handle_ping(),
            Packet::YourPing( p ) => self.net_controller.update_ping( p )
          }
        } else {
          break
        }
      }
    }

    if self.net_controller.poke_sync() {
      self.send_controlled_hero_sync();
    }

    // Update all the heroes
    for hero in self.heroes.iter_mut() {
      hero.update( delta_time );
    }

    self.controller.refresh();
  }

  fn draw( &self, w : &PistonWindow ) {
    w.draw_2d( |c, g| {
      clear( [1.0; 4], g );

      for hero in &self.heroes {
        ellipse( hero.color
               , [ hero.entity.pos.x as f64, hero.entity.pos.y as f64, 10.0, 10.0 ]
               , c.transform, g );
      }
    } );
  }
}

fn main() {
  let mut settings : GameSettings = Default::default();

  settings.host = std::env::args().skip(1).next();
  println!("Host: {:?}",settings.host );

  let nc = settings.make_net_controller();
  let mut window = settings.make_window();

  window.set_max_fps( 60 );
  window.set_ups( 120 );

  let mut game = Game::new( nc );

  for e in window {

    if let Some( xy ) = e.mouse_cursor_args() {
      game.update_cursor( xy[0], xy[1] );
    }

    if let Some( b ) = e.press_args() {
      game.input_press( b );
    }

    if let Some( ua ) = e.update_args() {
      game.update( ua.dt );
    }

    game.draw( &e )
  }
}