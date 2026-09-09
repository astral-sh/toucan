unsigned swap(unsigned value) { __asm__("bswap %0" : "+r" (value)); return value; }
void copy(int *p, int x) { __asm__ volatile("mov %[src],%[dst]" : [dst] "=m" (*p) : [src] "r" (x) : "memory", "cc"); }
