@0xf3f29e515cef1375;

struct Connect {
}

struct InitialSync {
  yourId @0 : UInt8;
}

struct Packet {
  union {
    instruction @0 : Instruction;
    sync        @1 : Sync;
    ping        @2 : Void;
    yourPing    @3 : UInt32;
  }
}

struct Instruction {
  heroId @0 : UInt8;
  moveTo     : union {
    nowhere @1 : Void;
    target  @2 : Vec2;
  }
}

struct Vec2 {
  x @0 : Float32;
  y @1 : Float32;
}

struct Sync {
  heroId    @0 : UInt8;
  syncFrame @1 : Hero;
}

struct Hero {
  entity     @0 : Entity;
  color      @1 : Color;
  targetPos    : union {
    nowhere @2 : Void;
    target  @3 : Vec2;
  }
}

struct Entity {
  pos    @0 : Vec2;
  vel    @1 : Vec2;
  hitbox @2 : Hitbox;
}

struct Hitbox {
  union {
    none   @0 : Void;
    circle @1 : Float32;
  }
}

struct Color {
  r @0 : Float32;
  g @1 : Float32;
  b @2 : Float32;
  a @3 : Float32;
}
