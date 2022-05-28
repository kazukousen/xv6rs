use crate::param::PHYSTOP;
use crate::spinlock::SpinLock;
use alloc::alloc::Layout;

use self::linked_list::LinkedListAllocator;

mod linked_list;

#[global_allocator]
pub static ALLOCATOR: SpinLock<LinkedListAllocator> =
    SpinLock::new(LinkedListAllocator::new(), "allocator");

#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
    panic!("allocation error: {:?}", layout)
}

pub fn heap_init() {
    extern "C" {
        fn end(); // see kernel.ld linker script
    }
    let heap_start: usize = end as usize;
    unsafe {
        ALLOCATOR.lock().init(heap_start, PHYSTOP - heap_start);
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;

    #[test_case]
    fn simple_allocation() {
        let v1 = Box::new(41);
        let v2 = Box::new(13);
        assert_eq!(41, *v1);
        assert_eq!(13, *v2);
    }
}
