extern crate graphics;
extern crate piston_window;
extern crate cgmath;
extern crate rand;
extern crate capnp;
extern crate time;

mod packet;
mod entity;
mod udpstream;

use piston_window::*;
use std::default::Default;
use std::net::{TcpListener, TcpStream, UdpSocket, SocketAddr, ToSocketAddrs};
use std::thread;
use std::io::{BufReader, BufWriter, Read, Write};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use time::{Timespec, get_time};

use packet::{Packet, InstructionPacket, SyncPacket, net_thread};
use entity::{Hero, Pos2, Vec2};
use udpstream::{BufUdpStream, UdpStream};

mod packets_capnp {
  include!( concat!( env!("OUT_DIR"), "/packets_capnp.rs" ) );
}

const GAME_TITLE : &'static str = "Mubah - v0.1.0";

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