void f(int n){typedef int A[n++];_Atomic(A*) p;
A*q=__c11_atomic_load(&p,0);q=__c11_atomic_fetch_add(&p,1,5);
q=__c11_atomic_exchange(&p,q,5);}
