.DEFAULT_GOAL := build

%:
	mkdir -p $(O)
	touch $(O)/.config

build:
	mkdir -p $(O)/images
	dd if=/dev/zero of=$(O)/images/rootfs.ext4 bs=1M count=1
