sudo cp chirpsounder_kuu.service /etc/systemd/system/chirpsounder.service
sudo systemctl daemon-reload
sudo systemctl start chirpsounder
sudo systemctl enable chirpsounder
sudo systemctl status chirpsounder
