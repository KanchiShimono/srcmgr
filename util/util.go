package util

import (
	"errors"
	"fmt"
)

func ShowExistError(msg string, err error) error {
	fmt.Println(msg)
	return err
}

func ShowNewError(msg string) error {
	err := errors.New(msg)
	fmt.Println(err)
	return err
}
