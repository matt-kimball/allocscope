#include <pthread.h>
#include <stdlib.h>

#define NUM_THREADS 8

void *allocate_block() {
    return malloc(64 * 1024);
}

void free_block(void *mem) {
    free(mem);
}

void *worker(void *arg) {
    for (int i = 0; i < 100; i++) {
        void *mem = allocate_block();
        free_block(mem);
    }

    return NULL;
}

int main() {
    pthread_t threads[NUM_THREADS];

    for (int i = 0; i < NUM_THREADS; i++) {
        pthread_create(&threads[i], NULL, worker, NULL);
    }

    for (int i = 0; i < NUM_THREADS; i++) {
        void *retval;
        pthread_join(threads[i], &retval);
    }

    return 0;
}
