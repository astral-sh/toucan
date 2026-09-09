typedef signed char S __attribute__((vector_size(16)));
typedef unsigned char U __attribute__((vector_size(16)));
int calls;
int left(void) { ++calls; return 3; }
int right(void) { ++calls; return 7; }
int run(void) {
    if (__builtin_elementwise_add_sat(2147483647,1)!=2147483647) return 1;
    if (__builtin_elementwise_sub_sat(-2147483647-1,1)!=(-2147483647-1)) return 2;
    if (__builtin_elementwise_add_sat(~0u,1u)!=~0u) return 3;
    if (__builtin_elementwise_sub_sat(0u,1u)!=0u) return 4;
    if (__builtin_elementwise_add_sat((signed char)127,(signed char)127)!=254) return 5;
    if (__builtin_elementwise_min(-1,1u)!=1u) return 6;
    if (__builtin_elementwise_max(left(),right())!=7 || calls!=2) return 7;
    S a={127,-128,100,-100},b={1,1,100,-100};
    S s=__builtin_elementwise_add_sat(a,b);
    if(s[0]!=127||s[1]!=-127||s[2]!=127||s[3]!=-128) return 8;
    s=__builtin_elementwise_sub_sat(a,b);
    if(s[0]!=126||s[1]!=-128||s[2]!=0||s[3]!=0) return 9;
    s=__builtin_elementwise_min(a,b);
    if(s[0]!=1||s[1]!=-128||s[2]!=100||s[3]!=-100) return 10;
    s=__builtin_elementwise_max(a,b);
    if(s[0]!=127||s[1]!=1||s[2]!=100||s[3]!=-100) return 11;
    U x={255,0,200},y={1,1,100};
    U u=__builtin_elementwise_add_sat(x,y);
    if(u[0]!=255||u[1]!=1||u[2]!=255) return 12;
    u=__builtin_elementwise_sub_sat(x,y);
    if(u[0]!=254||u[1]!=0||u[2]!=100) return 13;
    u=__builtin_elementwise_min(x,y);
    if(u[0]!=1||u[1]!=0||u[2]!=100) return 14;
    u=__builtin_elementwise_max(x,y);
    if(u[0]!=255||u[1]!=1||u[2]!=200) return 15;
    // __SIZEOF_INT128__ is absent on the Clang i686 target; keep all other
    // scalar and vector cases active there.
#if defined(__SIZEOF_INT128__)
    unsigned __int128 umax=(unsigned __int128)-1;
    __int128 imax=(__int128)(umax>>1), imin=-imax-1;
    if(__builtin_elementwise_add_sat(umax,(unsigned __int128)1)!=umax) return 16;
    if(__builtin_elementwise_sub_sat((unsigned __int128)0,(unsigned __int128)1)!=0) return 17;
    if(__builtin_elementwise_add_sat(imax,(__int128)1)!=imax) return 18;
    if(__builtin_elementwise_sub_sat(imin,(__int128)1)!=imin) return 19;
    if(__builtin_elementwise_min(imax,imin)!=imin||__builtin_elementwise_max(imax,imin)!=imax) return 20;
#endif
    if(__builtin_constant_p(__builtin_elementwise_add_sat(left(),right()))||calls!=2) return 21;
    return 0;
}
