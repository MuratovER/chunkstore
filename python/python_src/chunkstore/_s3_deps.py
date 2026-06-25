from __future__ import annotations

from dataclasses import dataclass
from typing import Any

_S3_INSTALL_HINT = "install chunkstore[s3]"


@dataclass(frozen=True, slots=True)
class Boto3Deps:
    boto3: Any
    Config: Any
    ClientError: Any


@dataclass(frozen=True, slots=True)
class AiobotocoreDeps:
    aio_session: Any
    Config: Any
    ClientError: Any


def require_boto3() -> Boto3Deps:
    try:
        import boto3
        from botocore.config import Config
        from botocore.exceptions import ClientError
    except ImportError as exc:
        raise RuntimeError(f"S3Backend requires boto3; {_S3_INSTALL_HINT}") from exc
    return Boto3Deps(boto3=boto3, Config=Config, ClientError=ClientError)


def require_aiobotocore() -> AiobotocoreDeps:
    try:
        import aiobotocore.session as aio_session
        from botocore.config import Config
        from botocore.exceptions import ClientError
    except ImportError as exc:
        raise RuntimeError(f"AsyncS3Backend requires aiobotocore; {_S3_INSTALL_HINT}") from exc
    return AiobotocoreDeps(aio_session=aio_session, Config=Config, ClientError=ClientError)
