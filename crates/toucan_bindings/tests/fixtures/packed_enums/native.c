#include "api.h"
#include <stdarg.h>
enum Byte byte_echo(enum Byte x){return x;}
enum SignedByte signed_next(enum SignedByte x){return x+1;}
enum Word word_previous(enum Word x){return x-1;}
enum SignedWord signed_word_next(enum SignedWord x){return x+1;}
struct Packet packet_update(struct Packet x){x.byte++;x.signed_byte++;x.word--;x.values[2]=199;return x;}
union Value union_previous(union Value x){x.word--;return x;}
enum Byte invoke(enum Byte(*callback)(enum Byte),enum Byte x){return callback(x);}
int sum_many(enum Byte a,enum Byte b,enum Byte c,enum Byte d,enum Byte e,enum Byte f,enum Byte g,enum Byte h,enum Byte i,enum Byte j,enum Byte k,enum Byte l){return a+b+c+d+e+f+g+h+i+j+k+l;}
int promoted(int marker,...){va_list args;va_start(args,marker);int a=va_arg(args,int),b=va_arg(args,int),c=va_arg(args,int),d=va_arg(args,int);va_end(args);return marker+a+b+c+d;}
int check_callbacks(int(*a)(enum Byte),int(*b)(enum SignedByte),int(*c)(enum Word),int(*d)(enum SignedWord)){
    for(int n=0;n<=255;n++)if(a(n)!=n)return 1;
    for(int n=-128;n<=127;n++)if(b(n)!=n)return 2;
    for(int n=0;n<=65535;n++)if(c(n)!=n)return 3;
    for(int n=-32768;n<=32767;n++)if(d(n)!=n)return 4;
    return 0;
}
int check_named_callbacks(int(*a)(enum Byte),int(*b)(enum SignedByte),int(*c)(enum Word),int(*d)(enum SignedWord)){
    return a(BYTE_MAX)!=255 || b(SIGNED_MIN)!=-128 || c(WORD_MAX)!=65535 || d(SWORD_MIN)!=-32768;
}
