int identity(int n) { return n; }
int deduced(int n) {
  typedef int A[n++];
  A a;
  __auto_type array = &a;
  __auto_type callback = identity;
  const __auto_type value = ({ __auto_type inner = callback(n); inner; });
  __auto_type text = "text";
  return value + sizeof *array + text[0];
}
