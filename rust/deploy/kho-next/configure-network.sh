#!/bin/bash
set -euo pipefail
[[ $(id -u) == 0 && $(hostname) == administrator-MIC-780W ]]
# Preserve the management connection and its addressing. Only spare ports join.
mkdir -p /root/kho-before-network
if [[ ! -d /root/kho-before-network/system-connections ]]; then
 cp -a /etc/NetworkManager/system-connections /etc/netplan /root/kho-before-network/
fi
for profile in 'Wired connection 2' 'Wired connection 3' netplan-enp131s0; do
 nmcli connection modify "$profile" connection.autoconnect no
done
if ! nmcli connection show kho-usrp >/dev/null 2>&1; then
nmcli connection add type bond ifname usrp0 con-name kho-usrp \
 bond.options 'mode=active-backup,miimon=100' ipv4.method manual ipv4.addresses 192.168.10.3/24 \
 ipv4.never-default yes ipv6.method disabled connection.autoconnect yes connection.autoconnect-slaves 1
fi
nmcli connection up kho-usrp
for port in enp129s0 enp130s0 enp131s0; do
 if ! nmcli connection show "kho-$port" >/dev/null 2>&1; then
 nmcli connection add type ethernet slave-type bond ifname "$port" con-name "kho-$port" master usrp0 \
 connection.autoconnect yes connection.autoconnect-priority 100
 fi
 nmcli connection up "kho-$port"
done
cat > /etc/sysctl.d/70-kho-usrp.conf <<'EOF'
net.core.rmem_max=536870912
net.core.wmem_max=536870912
net.core.netdev_max_backlog=10000
EOF
sysctl --system >/dev/null
cat > /etc/security/limits.d/70-kho-radio.conf <<'EOF'
administrator - rtprio 50
administrator - memlock 262144
EOF
ip -br address
cat /proc/net/bonding/usrp0
