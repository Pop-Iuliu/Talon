#include <stdint.h>
#include <stddef.h>

extern uint8_t SIGNALS[65536];

void hit_block(size_t block_id) {
    SIGNALS[block_id]++;
}

void target_function(const uint8_t *data, size_t size) {
    hit_block(100); 
    
    if (size < 5) return;
    hit_block(101);

    if (data[0] == 'T') {
        hit_block(102);
        
        if (data[1] == 'A') {
            hit_block(103);

            volatile int *crash_ptr = (int*)0;
            *crash_ptr = 42; 
        }
    }
}