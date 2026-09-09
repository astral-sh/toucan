extern _Thread_local int shared;
static __thread int hidden=7;
_Thread_local const char *text="thread";
void update(int n) {
    extern _Thread_local int shared;
    extern __thread int hidden;
    static _Thread_local int local[3]={1,2};
    static _Thread_local int (*pointer)[n];
    int *address=&shared;
    { static int shared=9; shared++; }
    *address=local[0]+hidden;
    pointer=0;
}
_Thread_local int shared=3;
