#![no_std]
#![no_main]

use core::{mem, ptr, slice, str::from_utf8_unchecked};

use xv6rs_user::{
    fstat::{stat, DirEnt, FileStat, InodeType, DIRSIZ},
    println, strlen,
    syscall::{sys_close, sys_exit, sys_fstat, sys_open, sys_read},
    Args,
};

#[no_mangle]
extern "C" fn _start(argc: i32, argv: &[&str]) {
    if argc < 2 {
        ls(".\0");
        sys_exit(0);
    }
    for arg in Args::new(argc, argv).skip(1) {
        ls(arg);
    }
    sys_exit(0);
}

fn ls(path: &str) {
    let fd = sys_open(&path, 0);
    if fd < 0 {
        println!("ls: open error");
        sys_exit(1);
    }

    let mut st = FileStat::uninit();
    if sys_fstat(fd, &mut st) < 0 {
        println!("ls: fstat error");
        sys_exit(1);
    }

    match st.typ {
        InodeType::Directory => {
            if path.len() > DIRSIZ {
                println!("ls: path too long");
                sys_exit(1);
            }

            let mut buf = [0u8; 512]; // sufficiently larger than the the max length of dirent name

            unsafe {
                ptr::copy_nonoverlapping(
                    path.as_bytes().as_ptr(),
                    &mut buf as *mut _,
                    path.len() - 1,
                )
            };

            buf[path.len() - 1] = b'/';

            let p: *mut u8 = unsafe { buf.as_mut_ptr().offset((path.len()).try_into().unwrap()) };

            let mut de = DirEnt::empty();
            let de_slice: &mut [u8] = unsafe {
                slice::from_raw_parts_mut(
                    &mut de as *mut DirEnt as *mut u8,
                    mem::size_of::<DirEnt>(),
                )
            };
            while sys_read(fd, de_slice) == mem::size_of::<DirEnt>().try_into().unwrap() {
                if de.inum == 0 {
                    continue;
                }

                unsafe { ptr::copy(de.name.as_ptr(), p, DIRSIZ) };

                if let Err(msg) = stat(unsafe { from_utf8_unchecked(&buf) }, &mut st) {
                    println!("ls: cannot stat: {}", msg);
                    continue;
                }

                let mut name = [0u8; DIRSIZ + 1];
                for i in 0..strlen(de.name.as_ptr()) {
                    name[i] = de.name[i];
                }
                for i in strlen(de.name.as_ptr())..=DIRSIZ {
                    name[i] = b' ';
                }
                println!(
                    "{} {:?} {} {}",
                    unsafe { from_utf8_unchecked(&name) },
                    st.typ,
                    st.inum,
                    st.size
                );
            }
        }
        InodeType::File => {
            println!("{} {:?} {} {}", path, st.typ, st.inum, st.size);
        }
        _ => {}
    }

    sys_close(fd);
}
