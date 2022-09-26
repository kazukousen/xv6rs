use alloc::boxed::Box;

const MBUF_SIZE: usize = 2048;

/// packet buffer management
#[repr(C, align(8))]
pub struct MBuf {
    head: usize,
    len: isize,
    // buffer
    buf: [u8; MBUF_SIZE],
}

// https://doc.rust-lang.org/nomicon/send-and-sync.html
// unsafe impl Send for MBuf {}

impl MBuf {
    pub fn alloc(headroom: usize) -> Box<Self> {
        if headroom > MBUF_SIZE {
            panic!("mbuf_alloc");
        }

        let mut mbuf = Box::new(Self {
            head: 0,
            len: 0,
            buf: [0u8; MBUF_SIZE],
        });
        mbuf.head = mbuf.buf.as_mut_ptr() as usize + headroom;
        mbuf
    }

    pub fn get_buf_head(&mut self) -> *mut u8 {
        self.head as *mut u8
    }

    pub fn get_len(&self) -> usize {
        self.len as usize
    }

    pub fn append(&mut self, len: usize) -> *mut u8 {
        let ret = self.head + (self.len as usize);
        self.len = self.len + (len as isize);
        if self.len > MBUF_SIZE as isize {
            panic!("mbuf_append");
        }

        ret as *mut u8
    }

    pub fn prepend<'a, T: Sized>(&mut self, len: usize) -> &'a mut T {
        self.head = self.head - len;
        if (self.head as *mut u8) < self.buf.as_mut_ptr() {
            panic!("mbuf_prepend: head is preceding bytes.");
        }
        self.len = self.len + (len as isize);
        unsafe { (self.head as *mut u8 as *mut T).as_mut() }.unwrap()
    }

    pub fn pop<'a, T: Sized>(&mut self, len: usize) -> &'a mut T {
        let ret = self.head;
        self.len = self.len - (len as isize);
        if self.len < 0 {
            // TODO
            panic!("mbuf_pop");
        }
        self.head = ret + len;
        unsafe { (ret as *mut u8 as *mut T).as_mut() }.unwrap()
    }

    pub fn trim(&mut self, len: usize) -> *mut u8 {
        if len > (self.len as usize) {
            // TODO
            panic!("mbuf_trim");
        }
        self.len = self.len - (len as isize);

        (self.head + (self.len as usize)) as *mut u8
    }
}
