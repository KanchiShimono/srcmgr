package main

type VCS interface {
	Clone(remoteRepo RemotoRepository, local string) (err error)
}
