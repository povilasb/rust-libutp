#!/bin/bash

# 1. gets libutp from github
# 2. compiles it
# 3. installs into the system - needs root permissions for this

libutp_version=2b364cbb0650bdab64a5de2abb4518f9f228ec44
expected_hash=0c7c324bd39c6f6862a7e1f02b9aaa020bb8a46231761308dcab90fd1503fa28
package=$libutp_version.zip

wget https://github.com/bittorrent/libutp/archive/$package
# actual_hash=`sha256sum $package | cut -d ' ' -f 1`
# if [ "$actual_hash" != "$expected_hash" ]; then
#     echo "Unexpected libutp package hash"
#     exit 1
# fi

unzip $package
cd libutp-$libutp_version
make
sudo cp libutp.so /usr/lib/
