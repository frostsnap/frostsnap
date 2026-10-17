use std::ffi::CStr;
use std::fs::File;
use std::io;
use std::os::unix::io::FromRawFd;

pub struct Pty {
    pub master: File,
    pub slave_path: String,
    _slave_file: File,
}

impl Pty {
    pub fn open() -> io::Result<Self> {
        let mut master_fd: libc::c_int = -1;
        let mut slave_fd: libc::c_int = -1;
        let mut name_buf = [0 as libc::c_char; 1024];

        let res = unsafe {
            libc::openpty(
                &mut master_fd,
                &mut slave_fd,
                name_buf.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };

        if res != 0 {
            return Err(io::Error::last_os_error());
        }

        let slave_path = unsafe { CStr::from_ptr(name_buf.as_ptr()) }
            .to_str()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            .to_string();

        unsafe {
            let mut termios: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(master_fd, &mut termios) != 0 {
                let err = io::Error::last_os_error();
                libc::close(master_fd);
                libc::close(slave_fd);
                return Err(err);
            }
            libc::cfmakeraw(&mut termios);
            if libc::tcsetattr(master_fd, libc::TCSANOW, &termios) != 0 {
                let err = io::Error::last_os_error();
                libc::close(master_fd);
                libc::close(slave_fd);
                return Err(err);
            }
            let flags = libc::fcntl(master_fd, libc::F_GETFL);
            if flags < 0 {
                let err = io::Error::last_os_error();
                libc::close(master_fd);
                libc::close(slave_fd);
                return Err(err);
            }
            if libc::fcntl(master_fd, libc::F_SETFL, flags | libc::O_NONBLOCK) != 0 {
                let err = io::Error::last_os_error();
                libc::close(master_fd);
                libc::close(slave_fd);
                return Err(err);
            }
        }

        let master = unsafe { File::from_raw_fd(master_fd) };

        let slave_file = unsafe { File::from_raw_fd(slave_fd) };

        Ok(Self {
            master,
            slave_path,
            _slave_file: slave_file,
        })
    }
}
