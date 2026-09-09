int printf(const char *, ...);
int events;
int bound(int x){events=events*10+x;return 2;}
int shape(a,b) int (*b)[bound(2)];int (*a)[bound(1)]; {return events;}
int prototype(int (*a)[bound(1)],int (*b)[bound(2)]){return events;}
double narrow(a,b) unsigned char a;float b;{return (double)a+(double)b;}
int callback(a,f) int a;int f(int);{return f(a);}
int next(int x){return x+1;}
int main(void){int a[2];int order=shape(&a,&a);double value=narrow(257,16777217.0);int called=callback(3,next);events=0;int proto=prototype(&a,&a);printf("order=%d narrow=%.17g callback=%d prototype=%d\n",order,value,called,proto);return (order!=12&&order!=21) | ((value!=16777217.0)<<1) | ((called!=4)<<2) | (((proto!=12&&proto!=21))<<3);}
