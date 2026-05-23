pub struct Routes;

impl Routes {
    pub const CREATE: i32 = 1;
    pub const JOIN: i32 = 2;
    pub const MESSAGE :i32 = 4;
}


pub struct WsCode;

impl WsCode {
    pub const JOIN: i32 =  2;
    pub const QUIT: i32 = 3;
    pub const MESSAGE :i32 = 4;
    
}

