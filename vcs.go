package main

import (
	"github.com/KanchiShimono/srcmgr/util"
)

type VCS interface {
	Clone(remoteRepo RemotoRepository, local string) (err error)
}

type Git struct{}

func (vcs *Git) Clone(remoteRepo RemotoRepository, local string) (err error) {
	return util.Run("git", "clone", remoteRepo.StringURL(), local)
}

type Mercurial struct{}

func (vcs *Mercurial) Clone(remoteRepo RemotoRepository, local string) (err error) {
	return util.Run("hg", "clone", remoteRepo.StringURL(), local)
}
