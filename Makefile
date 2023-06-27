service := omnisat-indexer-rs
port := 3445
version := 0.0.26
docker-org := pineappleworkshop
docker-registry := gcr.io
docker-image := pineappleworkshop/${service}:${version}
root := $(abspath $(shell pwd))

bootstrap:
	go mod init $(service)
	make init

init:
	go mod tidy

build:
	go build main.go

dev:
	go run main.go

test:
	go test -v ./...

docker-build:
	docker build -t $(docker-registry)/$(docker-image) .

docker-push:
	docker push $(docker-registry)/$(docker-image)

docker-run:
	@docker run -itp $(port):$(port)  $(docker-image)

# this directive is not working
docker-supabase-up:
	set -o allexport
	source docker/.env.example
	docker-compose -f docker/docker-compose.yml up

purge:
	go clean
	rm -rf $(root)/vendor

bumpversion-patch:
	bumpversion patch --allow-dirty

bumpversion-minor:
	bumpversion minor --allow-dirty

bumpversion-major:
	bumpversion major --allow-dirty
