cd /data0/
while true
do
    rsync -av --progress . j@mikromikko.sgo.fi:noire/kuu/
    sleep 120
done
