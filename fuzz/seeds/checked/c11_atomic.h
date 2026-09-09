int f(volatile _Atomic(int)*p,int*q,int order){
__c11_atomic_init(p,1);__c11_atomic_store(p,2,order);
__c11_atomic_exchange(p,3,order);
__c11_atomic_compare_exchange_strong(p,q,4,5,2);
__c11_atomic_compare_exchange_weak(p,q,5,5,2);
__c11_atomic_fetch_add(p,1,order);__c11_atomic_fetch_sub(p,1,order);
__c11_atomic_fetch_and(p,3,order);__c11_atomic_fetch_or(p,3,order);
__c11_atomic_fetch_xor(p,3,order);__c11_atomic_fetch_nand(p,3,order);
__c11_atomic_fetch_min(p,3,order);__c11_atomic_fetch_max(p,3,order);
__c11_atomic_thread_fence(order);__c11_atomic_signal_fence(order);
return __c11_atomic_load(p,order)+__c11_atomic_is_lock_free(order++);
}
