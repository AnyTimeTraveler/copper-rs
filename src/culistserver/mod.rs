use crate::curpc;
use crate::common::CircularQueue;
use std::mem::size_of;
use generic_array::typenum::U32;
use std::sync::Mutex;
use crate::culist::CuList;
use crate::culist::get_culist_from_shared_mem;

pub const NB_LIST: usize = 32; // TODO: derive 32 from U32.


type CuListLifoIds = CircularQueue<i32, U32>;

pub struct CuListSource {
    guarded_shared_lifo: Mutex<*mut CuListLifoIds>,
    base_id: i32,
    local_cus: Vec<*mut CuList>,
}


fn get_culists_serving_channel_id() -> i32 {
    return match shm::ftok!("./cubase/culists_serving_id\0") {
        Some(id) => id as i32,
        None => -1,
    };
}

pub fn get_base_channel_id() -> i32 {
    return match shm::ftok!("./cubase/base_culist_id\0") {
        Some(id) => id as i32,
        None => -1,
    };
}

fn init_local_culist_source() -> CuListSource {
    let mut channel_id = get_culists_serving_channel_id();
    let source: CuListSource;
    unsafe {
        source = CuListSource {
            guarded_shared_lifo: Mutex::new(&mut *(curpc::sharedmem::get_shared_mem(channel_id, size_of::<CuListLifoIds>()) as *mut CuListLifoIds)),
            base_id: get_base_channel_id(),
            local_cus: Vec::with_capacity(NB_LIST),
        };
    }
    return source;
}

pub(crate) fn get_culist_source() -> CuListSource {
    let mut source = init_local_culist_source();
    source.sync_local_culists();
    return source;
}


impl CuListSource {
    fn sync_local_culists(&mut self) {
        let mut id = self.base_id;
        for _ in 0..NB_LIST {
            let cu = get_culist_from_shared_mem(id);
            self.local_cus.push(cu);
            cu.shared_mem_id = id;
            id += 1;
        }
    }

    /// Only for the server at the beginning.
    pub fn start(&mut self) {
        let mut lifo_ids = self.guarded_shared_lifo.lock().unwrap();
        for index in 0..NB_LIST {
            let id = self.base_id + index as i32;
            unsafe {
                (**lifo_ids).push(id);
                (*self.local_cus[index]).init(id);
            }
        }
    }


    pub fn get_fresh_culist(&mut self) -> &mut CuList {
        let mutex = &mut self.guarded_shared_lifo;
        let result = mutex.lock();
        let mut lifo_ids = result.unwrap();
        let id = unsafe { *(**lifo_ids).pop().unwrap() };
        println!("Fresh culist: poped {}", id);
        let culist = unsafe { &mut *(self.local_cus[(id - self.base_id) as usize]) };
        println!("Internal culist state is id {}", culist.shared_mem_id);
        return culist;
    }

    pub fn get_culist(&mut self, id: i32) -> &mut CuList {
        return unsafe { &mut *(self.local_cus[(id - self.base_id) as usize]) };
    }

    pub fn recycle_culist(&mut self, id: i32) {
        let cu : &mut CuList = self.get_culist(id);
        cu.reset();
        let mut lifo_ids = self.guarded_shared_lifo.lock().unwrap();
        unsafe { (**lifo_ids).push(id) };
    }
}
