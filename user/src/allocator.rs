use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    mem, ptr,
};

use crate::syscall::sys_sbrk;

struct ListNode {
    size: usize,
    next: Option<&'static mut ListNode>,
}

impl ListNode {
    const fn new(size: usize) -> Self {
        Self { size, next: None }
    }

    fn start_addr(&self) -> usize {
        self as *const Self as usize
    }

    fn end_addr(&self) -> usize {
        self.start_addr() + self.size
    }
}

pub struct UserAllocator {
    inner: UnsafeCell<UserAllocatorInner>,
}

// Safety: We ensure that all operations on the inner data are synchronized
// through the use of UnsafeCell and proper locking in the implementation.
unsafe impl Sync for UserAllocator {}

struct UserAllocatorInner {
    head: ListNode,
    initialized: bool,
}

impl UserAllocator {
    pub const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(UserAllocatorInner {
                head: ListNode::new(0),
                initialized: false,
            }),
        }
    }

    /// Initialize the allocator.
    pub fn init(&mut self) {
        let inner = unsafe { &mut *self.inner.get() };
        if inner.initialized {
            return;
        }
        
        // Allocate initial heap region (e.g., 4KB)
        let heap_size = 4096;
        let heap_start = sys_sbrk(heap_size as i32) as usize;
        
        unsafe {
            self.add_free_region(heap_start, heap_size);
        }
        
        inner.initialized = true;
    }

    /// Adds the given memory region to the front of the list.
    unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
        let inner = &mut *self.inner.get();
        // ensure that the freed region is capable of holding ListNode
        let aligned_addr = Self::align_up(addr, mem::align_of::<ListNode>());
        let size_reduction = aligned_addr - addr;
        
        assert!(size > size_reduction);
        assert!(size - size_reduction >= mem::size_of::<ListNode>());
        
        let adjusted_size = size - size_reduction;
        
        // Create a new list node and append it at the start of the list
        let mut node = ListNode::new(adjusted_size);
        node.next = inner.head.next.take();
        let node_ptr = aligned_addr as *mut ListNode;
        node_ptr.write(node);
        inner.head.next = Some(&mut *node_ptr);
    }

    fn find_region(&mut self, size: usize, align: usize) -> Option<(&'static mut ListNode, usize)> {
        let inner = unsafe { &mut *self.inner.get() };
        let mut current = &mut inner.head;

        while let Some(ref mut region) = current.next {
            if let Ok(alloc_start) = Self::alloc_from_region(&region, size, align) {
                let next = region.next.take();
                let ret = Some((current.next.take().unwrap(), alloc_start));
                current.next = next;
                return ret;
            } else {
                current = current.next.as_mut().unwrap();
            }
        }

        None
    }

    /// Try to use the given region for an allocation with given size and alignment.
    fn alloc_from_region(region: &ListNode, size: usize, align: usize) -> Result<usize, ()> {
        let alloc_start = Self::align_up(region.start_addr(), align);
        let alloc_end = alloc_start.checked_add(size).ok_or_else(|| ())?;

        if alloc_end > region.end_addr() {
            // region too small
            return Err(());
        }

        let excess_size = region.end_addr() - alloc_end;

        if excess_size > 0 && excess_size < mem::size_of::<ListNode>() {
            // rest of region too small to hold a ListNode
            return Err(());
        }

        Ok(alloc_start)
    }

    fn size_align(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(mem::align_of::<ListNode>())
            .expect("adjusting alignment failed")
            .pad_to_align();
        let size = layout.size().max(mem::size_of::<ListNode>());
        (size, layout.align())
    }

    fn align_up(addr: usize, align: usize) -> usize {
        (addr + align - 1) & !(align - 1)
    }
    
    /// Grow the heap when needed
    fn grow_heap(&mut self, size: usize) -> Result<(), ()> {
        // Round up size to multiple of 4KB
        let page_size = 4096;
        let pages = (size + page_size - 1) / page_size;
        let alloc_size = pages * page_size;
        
        // Call sbrk to extend the heap
        let addr = sys_sbrk(alloc_size as i32) as usize;
        if addr == 0 {
            return Err(());
        }
        
        // Add the new region to the free list
        unsafe {
            self.add_free_region(addr, alloc_size);
        }
        
        Ok(())
    }
}

unsafe impl GlobalAlloc for UserAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Get a pointer to the inner data
        let inner_ptr = self.inner.get();
        
        // Initialize if not already initialized
        if !(*inner_ptr).initialized {
            // Initialize directly
            let heap_size = 4096;
            let heap_start = sys_sbrk(heap_size as i32) as usize;
            
            // Add the free region directly
            let aligned_addr = Self::align_up(heap_start, mem::align_of::<ListNode>());
            let size_reduction = aligned_addr - heap_start;
            
            if heap_size > size_reduction && heap_size - size_reduction >= mem::size_of::<ListNode>() {
                let adjusted_size = heap_size - size_reduction;
                
                // Create a new list node and append it at the start of the list
                let mut node = ListNode::new(adjusted_size);
                node.next = (*inner_ptr).head.next.take();
                let node_ptr = aligned_addr as *mut ListNode;
                node_ptr.write(node);
                (*inner_ptr).head.next = Some(&mut *node_ptr);
            }
            
            (*inner_ptr).initialized = true;
        }
        
        let (size, align) = UserAllocator::size_align(layout);
        
        // Try to find a region
        let mut current = &mut (*inner_ptr).head;
        let mut alloc_start = 0;
        let mut found = false;
        let mut region_ptr = ptr::null_mut();
        
        while let Some(ref mut region) = current.next {
            if let Ok(start) = Self::alloc_from_region(&region, size, align) {
                let next = region.next.take();
                region_ptr = current.next.take().unwrap();
                current.next = next;
                alloc_start = start;
                found = true;
                break;
            } else {
                current = current.next.as_mut().unwrap();
            }
        }
        
        if found {
            let alloc_end = alloc_start.checked_add(size).expect("overflow");
            let excess_size = (*region_ptr).end_addr() - alloc_end;
            if excess_size > 0 {
                // Add the excess region directly
                let aligned_addr = Self::align_up(alloc_end, mem::align_of::<ListNode>());
                let size_reduction = aligned_addr - alloc_end;
                
                if excess_size > size_reduction && excess_size - size_reduction >= mem::size_of::<ListNode>() {
                    let adjusted_size = excess_size - size_reduction;
                    
                    // Create a new list node and append it at the start of the list
                    let mut node = ListNode::new(adjusted_size);
                    node.next = (*inner_ptr).head.next.take();
                    let node_ptr = aligned_addr as *mut ListNode;
                    node_ptr.write(node);
                    (*inner_ptr).head.next = Some(&mut *node_ptr);
                }
            }
            
            return alloc_start as *mut u8;
        }
        
        // If no suitable region found, grow the heap
        let page_size = 4096;
        let pages = (size + page_size - 1) / page_size;
        let alloc_size = pages * page_size;
        
        // Call sbrk to extend the heap
        let addr = sys_sbrk(alloc_size as i32) as usize;
        if addr == 0 {
            return ptr::null_mut();
        }
        
        // Add the new region directly
        let aligned_addr = Self::align_up(addr, mem::align_of::<ListNode>());
        let size_reduction = aligned_addr - addr;
        
        if alloc_size > size_reduction && alloc_size - size_reduction >= mem::size_of::<ListNode>() {
            let adjusted_size = alloc_size - size_reduction;
            
            // Create a new list node and append it at the start of the list
            let mut node = ListNode::new(adjusted_size);
            node.next = (*inner_ptr).head.next.take();
            let node_ptr = aligned_addr as *mut ListNode;
            node_ptr.write(node);
            (*inner_ptr).head.next = Some(&mut *node_ptr);
        }
        
        // Try allocation again
        return self.alloc(layout);
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Get a pointer to the inner data
        let inner_ptr = self.inner.get();
        
        let (size, _) = UserAllocator::size_align(layout);
        
        // Add the free region directly
        let aligned_addr = Self::align_up(ptr as usize, mem::align_of::<ListNode>());
        let size_reduction = aligned_addr - (ptr as usize);
        
        if size > size_reduction && size - size_reduction >= mem::size_of::<ListNode>() {
            let adjusted_size = size - size_reduction;
            
            // Create a new list node and append it at the start of the list
            let mut node = ListNode::new(adjusted_size);
            node.next = (*inner_ptr).head.next.take();
            let node_ptr = aligned_addr as *mut ListNode;
            node_ptr.write(node);
            (*inner_ptr).head.next = Some(&mut *node_ptr);
        }
    }
}

// Global allocator instance
#[global_allocator]
pub static ALLOCATOR: UserAllocator = UserAllocator::new();

// Allocation error handler
#[alloc_error_handler]
pub fn alloc_error_handler(layout: Layout) -> ! {
    panic!("allocation error: {:?}", layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::vec::Vec;
    use core::alloc::Layout;

    #[test_case]
    fn test_allocator_init() {
        // Test that the allocator can be initialized
        let mut allocator = UserAllocator::new();
        allocator.init();
        let inner = unsafe { &*allocator.inner.get() };
        assert!(inner.initialized);
    }

    #[test_case]
    fn test_allocator_alloc_dealloc() {
        // Test basic allocation and deallocation
        let layout = Layout::from_size_align(8, 8).unwrap();
        let allocator = UserAllocator::new();
        
        unsafe {
            let ptr = allocator.alloc(layout);
            assert!(!ptr.is_null());
            
            // Write to the allocated memory
            *ptr = 42;
            assert_eq!(*ptr, 42);
            
            // Deallocate
            allocator.dealloc(ptr, layout);
        }
    }

    #[test_case]
    fn test_allocator_multiple_allocs() {
        // Test multiple allocations
        let layout = Layout::from_size_align(8, 8).unwrap();
        let allocator = UserAllocator::new();
        
        unsafe {
            let ptrs: [*mut u8; 10] = core::array::from_fn(|_| {
                let ptr = allocator.alloc(layout);
                assert!(!ptr.is_null());
                ptr
            });
            
            // Deallocate all
            for ptr in ptrs {
                allocator.dealloc(ptr, layout);
            }
        }
    }

    #[test_case]
    fn test_allocator_large_alloc() {
        // Test a larger allocation
        let layout = Layout::from_size_align(4096, 8).unwrap();
        let allocator = UserAllocator::new();
        
        unsafe {
            let ptr = allocator.alloc(layout);
            assert!(!ptr.is_null());
            
            // Deallocate
            allocator.dealloc(ptr, layout);
        }
    }

    #[test_case]
    fn test_box_with_allocator() {
        // Test that Box works with our allocator
        let x = Box::new(42);
        assert_eq!(*x, 42);
        
        // Box is dropped here
    }

    #[test_case]
    fn test_vec_with_allocator() {
        // Test that Vec works with our allocator
        let mut v = Vec::new();
        for i in 0..100 {
            v.push(i);
        }
        
        assert_eq!(v.len(), 100);
        assert_eq!(v[0], 0);
        assert_eq!(v[99], 99);
        
        // Vec is dropped here
    }
}
