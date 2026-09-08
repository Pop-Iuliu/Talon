#include <stdint.h>
#include <stddef.h>

extern uint8_t SIGNALS[65536];

void hit_block(size_t block_id) {
    SIGNALS[block_id]++;
}

void target_function(const uint8_t *data, size_t size) {
    hit_block(100); 
    
    if (size < 10) return;
    hit_block(101);

    if (data[0] == 'T') {
        hit_block(102);
        
        if (data[1] == 'A') {
            hit_block(103);
            
            if (data[2] == 'L') {
                hit_block(104);
                
                if (data[3] == 'O') {
                    hit_block(105);
                    
                    if (data[4] == 'N') {
                        hit_block(106);
                        
                        if (data[5] == 0x01) {
                            hit_block(107);
                            
                            if (data[6] == 0xDE && data[7] == 0xAD) {
                                hit_block(108);
                                
                                if (data[8] == 0xBE && data[9] == 0xEF) {
                                    hit_block(200); 
                                    volatile int *crash_ptr = (int*)0;
                                    *crash_ptr = 42; 
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}