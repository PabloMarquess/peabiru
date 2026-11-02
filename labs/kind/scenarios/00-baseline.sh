# shellcheck shell=bash disable=SC2034  # as variáveis são lidas pelo lib.sh (load_scenario)
TITLE="Linha de base: tudo funcionando"
LAYER="-"
AWS_ANALOGY="Pod do EKS falando com o banco sem nenhuma falha"
EXPECT_SVC="dns=ok tcp=ok app=ok"
EXPECT_RDS="dns=ok tcp=ok app=ok"
PEABIRU_SHOULD="Caminho completo até os dois bancos, sem achados."

inject() { :; }
revert() { :; }
