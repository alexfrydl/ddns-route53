build:
  cargo build

docker:
  sudo docker build . -t alexfrydl/ddns-route53
  sudo docker push alexfrydl/ddns-route53