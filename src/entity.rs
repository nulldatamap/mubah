use cgmath;
use cgmath::{Vector2, Point2, Point, Vector, EuclideanVector, FixedArray};
use packet::InstructionPacket;
use rand::{thread_rng, Rng};

pub type Vec2 = cgmath::Vector2<f32>;
pub type Pos2 = cgmath::Point2<f32>;

#[derive(Clone)]
pub enum Hitbox {
  None,
  Circle( f32 )
}

#[derive(Clone)]
pub struct Entity {
  pub pos    : Pos2,
  pub vel    : Vec2,
  pub hitbox : Hitbox
}

impl Entity {
  pub fn update( &mut self, delta_time : f64 ) {
    self.pos = self.pos.add_v( &self.vel.mul_s( delta_time as f32 ) );

    self.vel = Vec2::new( 0.0, 0.0 );
  }
}

#[derive(Clone)]
pub struct Hero {
  pub entity     : Entity,
  pub color      : [f32; 4],
  pub target_pos : Option<Pos2>
}

impl Hero {
  pub fn new( sp : Pos2 ) -> Hero {
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

  pub fn instruct( &mut self, instr : InstructionPacket ) {
    if let Some( p ) = instr.move_to {
      self.target_pos = Some( p );
    }
  }

  pub fn update( &mut self, delta_time : f64 ) {

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
