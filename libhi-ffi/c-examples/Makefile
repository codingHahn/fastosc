CFLAGS = -O1 -g3 -I ../../target/include
LDLIBS = -ldl -lm

EXAMPLES = reciever simple_reciever

all: $(EXAMPLES)

$(EXAMPLES): %: %.c ../../target/debug/liblibhiffi.so

../../target/debug/liblibhiffi.so:
	cd .. && cargo build

clean:
	rm -f $(EXAMPLES)

.PHONY: clean
