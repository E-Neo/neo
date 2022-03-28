CC = gcc -std=c17
CFLAGS = -Wall -Wextra

MKDIR = mkdir -p
RM = rm -rf

SRC = src
OUTPUT = output

MAIN = $(OUTPUT)/main
TEST_MAIN = $(OUTPUT)/test_main

release: $(filter-out $(SRC)/test_main.c, $(wildcard $(SRC)/*.c))
	$(MKDIR) $(OUTPUT)
	$(CC) $(CFLAGS) -O2 -o $(MAIN) $^

debug: $(filter-out $(SRC)/test_main.c, $(wildcard $(SRC)/*.c))
	$(MKDIR) $(OUTPUT)
	$(CC) $(CFLAGS) -g -o $(MAIN) $^

test: $(filter-out $(SRC)/main.c, $(wildcard $(SRC)/*.c))
	$(MKDIR) $(OUTPUT)
	$(CC) $(CFLAGS) -DTESTS -g -o $(TEST_MAIN) $^

clean:
	$(RM) $(OUTPUT)
