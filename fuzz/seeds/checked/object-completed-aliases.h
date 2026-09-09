typedef int First[4]; typedef int Second[]; extern First array; Second array={1,2}; typedef int(*CallbackA)(int); typedef int(*CallbackB)(); extern CallbackA callback; CallbackB callback;
