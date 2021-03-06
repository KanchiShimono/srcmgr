package main

import (
	"errors"
	"fmt"
	"net/url"
	"os/exec"
	"regexp"
	"strings"
)

type RemotoRepository interface {
	URL() (url *url.URL)
	StringURL() (url string)
	IsValid() bool
	UsrRepoNameFrom(names string) (usrname, reponame string)
	Format4UsrRepoNames() (names string)
	VCS() (vcs VCS)
}

type GitHub struct {
	url *url.URL
}

func (repo *GitHub) URL() (url *url.URL) {
	return repo.url
}

func (repo *GitHub) StringURL() (url string) {
	return repo.url.String()
}

func (repo *GitHub) IsValid() bool {
	re := regexp.MustCompile(`^(((https?|git):\/\/)?github\.com/)?([A-Za-z0-9_-]+/)?[A-Za-z0-9_.-]+(\.git)?$`)
	return re.Match([]byte(repo.url.String()))
}

// Divide names {username/reponame} to usernam, reponame
func (repo *GitHub) UsrRepoNameFrom(names string) (usrname, reponame string) {
	// If names don't have username like names = reponame only
	// Autocomplete username from git configuration
	if !hasUsrName(names) {
		gitusr, err := exec.Command("git", "config", "--get", "user.name").Output()
		if err != nil {
			fmt.Println("Git user name has not been set")
			return "", ""
		}
		// Remove white space from user name that got by git config
		// For example " First Last " to "FirstLast"
		usrStr := strings.Join(strings.Fields(string(gitusr)), "")
		names = usrStr + "/" + names
	}

	usrname = strings.Split(names, "/")[0]
	reponame = strings.Split(names, "/")[1]

	return usrname, reponame
}

// Format username/reponame from original "argument" URL
// names = username/reponame
func (repo *GitHub) Format4UsrRepoNames() (names string) {
	prefix := regexp.MustCompile(`^(((https?|git):\/\/)?github\.com\/)?`)
	suffix := regexp.MustCompile(`\.git$`)

	names = prefix.ReplaceAllString(repo.StringURL(), "")
	names = suffix.ReplaceAllString(names, "")

	return names
}

func (repo *GitHub) VCS() (vcs VCS) {
	return &Git{}
}

type Bitbucket struct {
	url *url.URL
}

func (repo *Bitbucket) URL() (url *url.URL) {
	return repo.url
}

func (repo *Bitbucket) StringURL() (url string) {
	return repo.url.String()
}

func (repo *Bitbucket) IsValid() bool {
	re := regexp.MustCompile(`^(((https?|ssh)://)?(hg@)?bitbucket\.org/)?([A-Za-z0-9_-]+/)?[A-Za-z0-9_.-]+(\.git)?$`)
	return re.Match([]byte(repo.url.String()))
}

func (repo *Bitbucket) UsrRepoNameFrom(names string) (usrname, reponame string) {
	usrname = strings.Split(names, "/")[0]
	reponame = strings.Split(names, "/")[1]

	return usrname, reponame
}

func (repo *Bitbucket) Format4UsrRepoNames() (names string) {
	prefix := regexp.MustCompile(`^(((https?|ssh)://)?(hg@)?bitbucket\.org/)?`)

	names = prefix.ReplaceAllString(repo.StringURL(), "")

	return names
}

func (repo *Bitbucket) VCS() (vcs VCS) {
	return &Mercurial{}
}

func hasUsrName(names string) bool {
	return regexp.MustCompile(`/`).Match([]byte(names))
}

func NewRemoteRepository(u interface{}) (repo RemotoRepository, err error) {
	// Converted URL if u is string type
	var cnvUrl *url.URL

	switch v := u.(type) {
	case string:
		cnvUrl, err = url.Parse(v)
	case *url.URL:
		cnvUrl, err = v, nil
	default:
		return nil, errors.New("URL argument is invalid type")
	}

	if err != nil {
		return nil, errors.New("Invalid URL")
	}

	host := cnvUrl.Hostname()

	switch host {
	case "github.com":
		return &GitHub{url: cnvUrl}, err
	case "bitbucket.org":
		return &Bitbucket{url: cnvUrl}, err
	default:
		cnvUrl.Scheme = "https"
		cnvUrl.Host = "github.com"
		return &GitHub{url: cnvUrl}, err
	}

}
