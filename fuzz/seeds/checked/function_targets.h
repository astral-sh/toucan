__attribute__((target("mmx"),always_inline)) inline int increment(int x){return x+1;}
__attribute__((target("no-mmx"))) int baseline(int x){if(0)return increment(x);return x;}
__attribute__((target("sse2"))) int call(int x){return increment(x);}
