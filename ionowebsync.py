import requests

def post_to_server(fname):
    # edit /etc/apache2/conf-available/upload-limit.conf
    # to enable ip!
    try:
        url="http://4.235.86.214/upload_h5.php"
        with open(fname, "rb") as f:
            r = requests.post(url, files={"file": f})
    except:
        print("couldn't post file %s to server"%(fname))
