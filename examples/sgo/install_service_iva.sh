sudo cp chirpsounder_iva.service /etc/systemd/system/chirpsounder.service
sudo systemctl daemon-reload
sudo systemctl start chirpsounder
sudo systemctl enable chirpsounder
sudo systemctl status chirpsounder
