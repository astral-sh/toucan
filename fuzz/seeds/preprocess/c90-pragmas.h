_Pragma("pack(push) // pragma comment")
#pragma pack(pop) // compilation and -E differ in GCC C90
#if 1 // compilation and -E differ in Clang C90
int selected;
#endif
