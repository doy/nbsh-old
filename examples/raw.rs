use std::io::Read;

fn main() {
    let _screen = crossterm::RawScreen::into_raw_mode().unwrap();
    loop {
        let stdin = std::io::stdin();
        let mut stdin = stdin.lock();
        let mut buf = [0; 1];
        let n = stdin.read(&mut buf).unwrap();
        if n > 0 {
            eprint!("got {}\r\n", buf[0]);
            if buf[0] == 4 {
                break;
            }
        } else {
            eprint!("got no bytes\r\n");
            break;
        }
    }
}
