typedef int I __attribute__((aligned(16)));
int value;
int f(volatile int *p, short input, int ignored) {
 int old=__sync_fetch_and_add(p,input,ignored++,sizeof(int[++ignored]));
 _Bool changed=__sync_bool_compare_and_swap(p,old,2,(void)0);
 __sync_lock_release(p,ignored++);
 __sync_synchronize();
 return old+changed;
}
_Static_assert(__builtin_constant_p(__sync_lock_test_and_set(&value,1))==0,"side effects");
