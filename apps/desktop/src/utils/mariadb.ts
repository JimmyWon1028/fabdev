import erpConfig from '../../../../resources/mariadb/erp.cnf?raw'

export const EMPTY_MARIADB_CONFIG = '[mariadbd]\n\n'
export const ERP_MARIADB_CONFIG = erpConfig.endsWith('\n') ? erpConfig : `${erpConfig}\n`
