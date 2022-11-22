#!/usr/bin/bash

while true
do
    rsync -e "ssh -i ~/.ssh/id_rsa" -av --progress /data0/2* j@mikromikko.sgo.fi:noire/iva/
done

